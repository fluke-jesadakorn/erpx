//! PostgreSQL owner-fact adapter for native Projects attachment targets.
//!
//! Reads only the Projects-owned project row. No attachment set, link, source,
//! event, scan or storage row is read or written here; Files owns that ledger.
use crate::application::project_attachment::{
    ProjectAttachmentCommandRepository, ProjectAttachmentFactsRepository, ProjectAttachmentFuture,
    ProjectAttachmentReceiptPlan, StoredProjectAttachmentReceipt,
};
use crate::domain::project_attachment::{ProjectAttachmentTargetFacts, idempotency_conflict};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn serializable_checked(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PROJECT_ATTACHMENT_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

pub struct PgProjectAttachmentFacts<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectAttachmentFacts<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }
}

fn sql_error(error: sqlx::Error) -> Error {
    let conflict = error
        .as_database_error()
        .and_then(|database| database.code())
        .is_some_and(|code| code == "23505");
    if conflict {
        return idempotency_conflict().into();
    }
    error.into()
}

/// Projects-owned idempotency ledger. The caller's retained serializable unit
/// writes the receipt after the Files bind and project version step so a replay
/// can never bind twice or step the project version twice.
pub struct PgProjectAttachmentCommandReceipts<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectAttachmentCommandReceipts<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn stored_receipt(
        &mut self,
        key: &str,
    ) -> Result<Option<StoredProjectAttachmentReceipt>, Error> {
        serializable_checked(&mut *self.connection)?;
        sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT operation,record_id,request_hash,result_json FROM command_receipts \
             WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await
        .map(|row| {
            row.map(|(operation, record_id, request_fingerprint, result_json)| {
                StoredProjectAttachmentReceipt {
                    operation,
                    record_id,
                    request_fingerprint,
                    result_json,
                }
            })
        })
        .map_err(Into::into)
    }

    pub async fn record_receipt(
        &mut self,
        plan: &ProjectAttachmentReceiptPlan,
    ) -> Result<(), Error> {
        serializable_checked(&mut *self.connection)?;
        let result_json = serde_json::to_string(&plan.receipt).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "PROJECT_ATTACHMENT_RECEIPT_INVALID",
                "The Project file receipt could not be retained.",
            )
        })?;
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&plan.key)
        .bind(&plan.operation)
        .bind(&plan.actor_id)
        .bind(&plan.request_fingerprint)
        .bind(&plan.set_id)
        .bind(result_json)
        .bind(plan.now)
        .bind(&plan.authority_json)
        .execute(&mut *self.connection)
        .await
        .map_err(sql_error)?;
        Ok(())
    }
}

impl ProjectAttachmentCommandRepository for PgProjectAttachmentCommandReceipts<'_> {
    type Error = Error;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<StoredProjectAttachmentReceipt>, Self::Error> {
        Box::pin(async move { PgProjectAttachmentCommandReceipts::stored_receipt(self, key).await })
    }

    fn record_receipt<'a>(
        &'a mut self,
        plan: &'a ProjectAttachmentReceiptPlan,
    ) -> ProjectAttachmentFuture<'a, (), Self::Error> {
        Box::pin(
            async move { PgProjectAttachmentCommandReceipts::record_receipt(self, plan).await },
        )
    }
}

impl ProjectAttachmentFactsRepository for PgProjectAttachmentFacts<'_> {
    type Error = Error;

    fn target<'a>(
        &'a mut self,
        company: &'a CompanyId,
        target_type: &'a str,
        target_id: &'a str,
        user_id: &'a str,
    ) -> ProjectAttachmentFuture<'a, Option<ProjectAttachmentTargetFacts>, Self::Error> {
        Box::pin(async move {
            serializable_checked(&mut *self.connection)?;
            if target_type != "project" {
                return Ok(None);
            }
            let row: Option<(String, String, String, String, i64, bool, bool)> = sqlx::query_as(
                "SELECT p.company_id,p.id,p.owner_user_id,p.lifecycle_status,p.version,\
                 iam_same_person(p.owner_user_id,$3,p.company_id) AS is_owner,\
                 EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id \
                   AND iam_same_person(m.user_id,$3,p.company_id)) AS is_member \
                 FROM staff_projects p \
                 WHERE p.id=$1 AND p.company_id=$2 \
                   AND (iam_same_person(p.owner_user_id,$3,p.company_id) \
                     OR EXISTS(SELECT 1 FROM staff_project_members m2 WHERE m2.project_id=p.id \
                       AND iam_same_person(m2.user_id,$3,p.company_id))) \
                 FOR UPDATE OF p",
            )
            .bind(target_id)
            .bind(company.as_str())
            .bind(user_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(row.map(
                |(
                    company_id,
                    target_id,
                    owner_user_id,
                    lifecycle_status,
                    version,
                    is_owner,
                    is_member,
                )| ProjectAttachmentTargetFacts {
                    company_id,
                    target_id,
                    owner_user_id,
                    lifecycle_status,
                    version,
                    is_owner,
                    is_member,
                },
            ))
        })
    }
}
