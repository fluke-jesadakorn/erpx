//! Operations-owned queue reads. Only `operations_queue_policies`,
//! `operations_exceptions` and `operations_exception_events` are read here.
//! Records source visibility, People labels, IAM and Workflow stay public ports
//! owned by their crates.
use crate::application::queue_read::{OperationsReadFuture, OperationsReadRepository};
use crate::domain::queue_read::{
    MAX_EXCEPTIONS, MAX_HISTORY, MAX_POLICIES, OperationsEventRow, WorkspaceData, read_limit,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("OPERATIONS_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("OPERATIONS_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub async fn lock_operations_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE operations_queue_policies,operations_exceptions,operations_exception_events IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

async fn workspace_image(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<String, Error> {
    let value: Option<String> = sqlx::query_scalar(
        "WITH exceptions AS MATERIALIZED (SELECT * FROM operations_exceptions WHERE company_id=$1 AND source_kind<>'field_service_order' AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3) ORDER BY updated_at DESC,id LIMIT 201),policies AS MATERIALIZED (SELECT * FROM operations_queue_policies WHERE company_id=$1 AND NOT (jsonb_typeof(source_kinds_json::jsonb)='array' AND source_kinds_json::jsonb<>'[]'::jsonb AND source_kinds_json::jsonb <@ '[\"field_service_order\"]'::jsonb) ORDER BY area,revision DESC,id LIMIT 501),payload AS (SELECT jsonb_build_object('exceptions',coalesce((SELECT jsonb_agg(to_jsonb(exceptions) ORDER BY updated_at DESC,id) FROM exceptions),'[]'::jsonb),'policies',coalesce((SELECT jsonb_agg(to_jsonb(policies) ORDER BY area,revision DESC,id) FROM policies),'[]'::jsonb))::text value) SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM payload",
    )
    .bind(company_id)
    .bind(branch_id)
    .bind(department_id)
    .fetch_one(c)
    .await?;
    value.ok_or_else(|| read_limit("The Operations queue exceeds its bounded read.").into())
}

async fn history_image(c: &mut PgConnection, exception_id: &str) -> Result<String, Error> {
    let value: Option<String> = sqlx::query_scalar(
        "WITH events AS MATERIALIZED (SELECT * FROM operations_exception_events WHERE exception_id=$1 ORDER BY version,id LIMIT 501),payload AS (SELECT coalesce(jsonb_agg(to_jsonb(events) ORDER BY version,id),'[]'::jsonb)::text value FROM events) SELECT CASE WHEN octet_length(value)<=2097152 THEN value END FROM payload",
    )
    .bind(exception_id)
    .fetch_one(c)
    .await?;
    value.ok_or_else(|| read_limit("The Operations history exceeds its bounded read.").into())
}

pub async fn read_operations_workspace(
    c: &mut PgConnection,
    company_id: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> Result<WorkspaceData, Error> {
    serializable(c).await?;
    lock_operations_read_sources(c).await?;
    let image = workspace_image(c, company_id, branch_id, department_id).await?;
    let data: WorkspaceData = serde_json::from_str(&image).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_READ_INVALID",
            "The Operations queue could not be read.",
        )
    })?;
    if data.exceptions.len() > MAX_EXCEPTIONS || data.policies.len() > MAX_POLICIES {
        return Err(read_limit("The Operations queue exceeds its bounded read.").into());
    }
    Ok(data)
}

pub async fn read_operations_history(
    c: &mut PgConnection,
    exception_id: &str,
) -> Result<Vec<OperationsEventRow>, Error> {
    serializable(c).await?;
    lock_operations_read_sources(c).await?;
    let image = history_image(c, exception_id).await?;
    let rows: Vec<OperationsEventRow> = serde_json::from_str(&image).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "OPERATIONS_READ_INVALID",
            "The Operations history could not be read.",
        )
    })?;
    if rows.len() > MAX_HISTORY {
        return Err(read_limit("The Operations history exceeds its bounded read.").into());
    }
    Ok(rows)
}

/// SQL repository adapter bound to the caller's retained serializable unit.
pub struct PgOperationsRead<'a> {
    connection: &'a mut PgConnection,
}
impl<'a> PgOperationsRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}
impl OperationsReadRepository for PgOperationsRead<'_> {
    type Error = Error;
    fn workspace<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: Option<&'a str>,
        department_id: Option<&'a str>,
    ) -> OperationsReadFuture<'a, WorkspaceData, Self::Error> {
        Box::pin(async move {
            read_operations_workspace(self.connection, company_id, branch_id, department_id).await
        })
    }
    fn history<'a>(
        &'a mut self,
        exception_id: &'a str,
    ) -> OperationsReadFuture<'a, Vec<OperationsEventRow>, Self::Error> {
        Box::pin(async move { read_operations_history(self.connection, exception_id).await })
    }
}
