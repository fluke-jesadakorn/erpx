//! PostgreSQL adapter for the native Records registry list/search/page read.
//!
//! SQL is Records-owned: only `workspace_records` is selected. Current IAM is
//! re-read through the existing assignments/IAM predicates used by the
//! retained owner. Cross-owner source admission is not performed here; the
//! caller supplies the public guard port. The caller's serializable read-only
//! Holding transaction is required and no commit is performed.
use crate::adapters::record_context::{AUTHORITY, map_record};
use crate::application::record_registry::RecordRegistryRepository;
use crate::domain::record_context::{RecordContextActor, RecordRow};
use crate::domain::record_registry::MAX_FETCH;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

const FORWARD: &str = "SELECT module_id,id,section_id,title,description,status,fields_json,\
 company_id,branch_id,department_id,version,created_by,updated_at \
 FROM workspace_records WHERE archived_at IS NULL AND company_id=$1 AND module_id=$2 \
 AND section_id=$3 \
 AND (($5::text IS NOT NULL AND department_id=$5::text) \
   OR ($5::text IS NULL AND $4::text IS NOT NULL AND branch_id=$4::text) \
   OR ($5::text IS NULL AND $4::text IS NULL)) \
 AND ($6::text IS NULL OR strpos(lower(title || ' ' || COALESCE(description,'')), \
   lower($6::text)) > 0) \
 AND ($7::bigint IS NULL OR (updated_at,id) < ($7::bigint,$8::text)) \
 ORDER BY updated_at DESC,id DESC LIMIT $9";

const REVERSE: &str = "SELECT module_id,id,section_id,title,description,status,fields_json,\
 company_id,branch_id,department_id,version,created_by,updated_at \
 FROM workspace_records WHERE archived_at IS NULL AND company_id=$1 AND module_id=$2 \
 AND section_id=$3 \
 AND (($5::text IS NOT NULL AND department_id=$5::text) \
   OR ($5::text IS NULL AND $4::text IS NOT NULL AND branch_id=$4::text) \
   OR ($5::text IS NULL AND $4::text IS NULL)) \
 AND ($6::text IS NULL OR strpos(lower(title || ' ' || COALESCE(description,'')), \
   lower($6::text)) > 0) \
 AND ($7::bigint IS NULL OR (updated_at,id) > ($7::bigint,$8::text)) \
 ORDER BY updated_at ASC,id ASC LIMIT $9";

pub struct PgRecordRegistry<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgRecordRegistry<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("RECORDS_REGISTRY_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("RECORDS_REGISTRY_SERIALIZABLE_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_read_only")
            .fetch_one(&mut *self.connection)
            .await?
            != "on"
        {
            return Err(PersistenceError::new("RECORDS_REGISTRY_READ_ONLY_REQUIRED").into());
        }
        Ok(())
    }
}

impl RecordRegistryRepository for PgRecordRegistry<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        now: i64,
    ) -> crate::application::record_registry::RecordRegistryFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar::<_, bool>(AUTHORITY)
                .bind(&actor.assignment_id)
                .bind(&actor.user_id)
                .bind(&actor.company_id)
                .bind(now)
                .bind(&actor.role)
                .bind(actor.assignment_version)
                .bind(actor.branch_id.as_deref())
                .bind(actor.department_id.as_deref())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn read_rows<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        section_id: &'a str,
        q: Option<&'a str>,
        after_key: Option<i64>,
        after_id: Option<&'a str>,
        descending: bool,
        limit: usize,
    ) -> crate::application::record_registry::RecordRegistryFuture<'a, Vec<RecordRow>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            if !(1..=MAX_FETCH).contains(&limit) {
                return Err(PersistenceError::new("RECORDS_REGISTRY_LIMIT_INVALID").into());
            }
            if after_key.is_some() != after_id.is_some() {
                return Err(PersistenceError::new("RECORDS_REGISTRY_CURSOR_INVALID").into());
            }
            let rows = sqlx::query(if descending { FORWARD } else { REVERSE })
                .bind(&actor.company_id)
                .bind(module_id)
                .bind(section_id)
                .bind(actor.branch_id.as_deref())
                .bind(actor.department_id.as_deref())
                .bind(q)
                .bind(after_key)
                .bind(after_id)
                .bind(limit as i64)
                .fetch_all(&mut *self.connection)
                .await?;
            rows.iter()
                .map(map_record)
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }
}
