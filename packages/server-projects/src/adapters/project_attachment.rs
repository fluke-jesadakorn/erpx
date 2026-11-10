//! PostgreSQL owner-fact adapter for native Projects attachment targets.
//!
//! Reads only the Projects-owned project row. No attachment set, link, source,
//! event, scan or storage row is read or written here; Files owns that ledger.
use crate::application::project_attachment::{
    ProjectAttachmentFactsRepository, ProjectAttachmentFuture,
};
use crate::domain::project_attachment::ProjectAttachmentTargetFacts;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
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
