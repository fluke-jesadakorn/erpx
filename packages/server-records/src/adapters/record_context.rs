//! PostgreSQL adapter for the native Records context read.
//!
//! SQL is Records-owned: `workspace_records`, `record_reference_edges`,
//! `record_principal_mentions`, `record_field_suggestions` and
//! `workspace_record_transitions`. Current IAM is re-read through the existing
//! assignments/IAM functions used by the retained owner. Cross-owner record
//! admission is not performed here; the caller supplies the public guard port.
//! The caller's serializable Holding transaction is required and no commit is
//! performed.
use crate::application::record_context::RecordContextRepository;
use crate::domain::record_context::{
    BacklinkEdge, HistoryRow, LinkTarget, MentionRow, RecordContextActor, RecordRow, ReferenceEdge,
    SuggestionRow,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub(crate) const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id \
 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'records.read') \
 AND m.status='active' \
 AND ($5::text IS NULL OR a.role=$5::text) \
 AND ($6::bigint IS NULL OR a.version=$6::bigint) \
 AND (a.scope_kind='holding' \
   OR (a.scope_kind='branch' AND a.scope_node_id=$7::text) \
   OR (a.scope_kind='department' AND a.scope_node_id=$8::text)))";

const SCOPED_SELECT: &str = "SELECT module_id,id,section_id,title,description,status,fields_json,\
 company_id,branch_id,department_id,version,created_by,updated_at \
 FROM workspace_records WHERE archived_at IS NULL AND company_id=$1 AND module_id=$2 AND id=$3 \
 AND (($5::text IS NOT NULL AND department_id=$5::text) \
   OR ($5::text IS NULL AND $4::text IS NOT NULL AND branch_id=$4::text) \
   OR ($5::text IS NULL AND $4::text IS NULL))";

pub(crate) fn map_record(row: &sqlx::postgres::PgRow) -> Result<RecordRow, sqlx::Error> {
    Ok(RecordRow {
        module_id: row.try_get("module_id")?,
        id: row.try_get("id")?,
        section_id: row.try_get("section_id")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        status: row.try_get("status")?,
        fields_json: row.try_get("fields_json")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub struct PgRecordContext<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgRecordContext<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("RECORDS_CONTEXT_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("RECORDS_CONTEXT_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn scoped_row(
        &mut self,
        actor: &RecordContextActor,
        module_id: &str,
        id: &str,
    ) -> Result<Option<RecordRow>, Error> {
        let row = sqlx::query(SCOPED_SELECT)
            .bind(&actor.company_id)
            .bind(module_id)
            .bind(id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.as_ref().map(map_record).transpose().map_err(Into::into)
    }

    /// One bounded timeline page for the drawer detail read. The caller asks
    /// for one row over the page size to detect a following page.
    pub(crate) async fn history_offset(
        &mut self,
        module_id: &str,
        id: &str,
        offset: i64,
        limit: usize,
    ) -> Result<Vec<HistoryRow>, Error> {
        self.ready().await?;
        if offset < 0 || !(1..=crate::domain::record_context::MAX_HISTORY + 1).contains(&limit) {
            return Err(PersistenceError::new("RECORDS_DETAILS_HISTORY_BOUND_INVALID").into());
        }
        let rows = sqlx::query(
            "SELECT from_status,to_status,action,note,actor_id,record_version,created_at \
             FROM workspace_record_transitions WHERE module_id=$1 AND record_id=$2 \
             ORDER BY created_at DESC LIMIT $3 OFFSET $4",
        )
        .bind(module_id)
        .bind(id)
        .bind(limit as i64)
        .bind(offset)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(HistoryRow {
                    from_status: row.try_get("from_status")?,
                    to_status: row.try_get("to_status")?,
                    action: row.try_get("action")?,
                    note: row.try_get("note")?,
                    actor_id: row.try_get("actor_id")?,
                    record_version: row.try_get("record_version")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(Into::into)
    }
}

impl RecordContextRepository for PgRecordContext<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        now: i64,
    ) -> crate::application::record_context::RecordContextFuture<'a, bool, Self::Error> {
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

    fn record<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Option<RecordRow>, Self::Error>
    {
        Box::pin(async move { self.scoped_row(actor, module_id, id).await })
    }

    fn references<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Vec<ReferenceEdge>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT target_module,target_id,kind,ordinal FROM record_reference_edges \
                 WHERE host_module=$1 AND host_id=$2 ORDER BY ordinal LIMIT 51",
            )
            .bind(module_id)
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(ReferenceEdge {
                        target_module: row.try_get("target_module")?,
                        target_id: row.try_get("target_id")?,
                        kind: row.try_get("kind")?,
                        ordinal: row.try_get("ordinal")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn backlinks<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Vec<BacklinkEdge>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT host_module,host_id,kind FROM record_reference_edges \
                 WHERE target_module=$1 AND target_id=$2 ORDER BY created_at DESC LIMIT 51",
            )
            .bind(module_id)
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(BacklinkEdge {
                        host_module: row.try_get("host_module")?,
                        host_id: row.try_get("host_id")?,
                        kind: row.try_get("kind")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn mentions<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Vec<MentionRow>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT target_kind,target_id,target_label,created_at \
                 FROM record_principal_mentions WHERE host_module=$1 AND host_id=$2 \
                 ORDER BY created_at LIMIT 201",
            )
            .bind(module_id)
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(MentionRow {
                        target_kind: row.try_get("target_kind")?,
                        target_id: row.try_get("target_id")?,
                        target_label: row.try_get("target_label")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn suggestions<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Vec<SuggestionRow>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,field_key,proposed_value_json,source_versions_json,provenance_json,\
                 state,created_at FROM record_field_suggestions \
                 WHERE module_id=$1 AND record_id=$2 ORDER BY created_at DESC LIMIT 21",
            )
            .bind(module_id)
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(SuggestionRow {
                        id: row.try_get("id")?,
                        field_key: row.try_get("field_key")?,
                        proposed_value_json: row.try_get("proposed_value_json")?,
                        source_versions_json: row.try_get("source_versions_json")?,
                        provenance_json: row.try_get("provenance_json")?,
                        state: row.try_get("state")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn history<'a>(
        &'a mut self,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Vec<HistoryRow>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT from_status,to_status,action,note,actor_id,record_version,created_at \
                 FROM workspace_record_transitions WHERE module_id=$1 AND record_id=$2 \
                 ORDER BY created_at DESC LIMIT 21",
            )
            .bind(module_id)
            .bind(id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(HistoryRow {
                        from_status: row.try_get("from_status")?,
                        to_status: row.try_get("to_status")?,
                        action: row.try_get("action")?,
                        note: row.try_get("note")?,
                        actor_id: row.try_get("actor_id")?,
                        record_version: row.try_get("record_version")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn link_target<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        id: &'a str,
    ) -> crate::application::record_context::RecordContextFuture<'a, Option<LinkTarget>, Self::Error>
    {
        Box::pin(async move {
            self.ready().await?;
            self.scoped_row(actor, module_id, id).await.map(|row| {
                row.map(|row| LinkTarget {
                    title: row.title,
                    version: row.version,
                })
            })
        })
    }
}
