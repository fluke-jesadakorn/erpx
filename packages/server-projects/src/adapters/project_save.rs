//! PostgreSQL adapter for native Projects create/edit commands.
//!
//! Only Projects-owned project/member tables plus the shared Workflow receipt,
//! audit and outbox rows are written. The caller owns the retained serializable
//! Holding transaction; the write re-locks the project and revalidates the
//! current owner, lifecycle and version before it commits the complete
//! membership, receipt and audit.
use crate::application::project_save::{ProjectSaveFuture, SaveProjectRepository};
use crate::domain::project_commands::ProjectCommandProject;
use crate::domain::project_save::{
    SaveProjectPlan, SaveProjectReceipt, StoredProjectReceipt, idempotency_conflict,
    missing_project, source_changed, structure_frozen, version_conflict,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("PROJECT_COMMAND_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}

async fn serializable_checked(connection: &mut PgConnection) -> Result<(), Error> {
    serializable(connection)?;
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("PROJECT_COMMAND_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn sql_error(error: sqlx::Error) -> Error {
    let Some(database) = error.as_database_error() else {
        return error.into();
    };
    let code = match database.code().as_deref() {
        Some("23505") => Some(idempotency_conflict()),
        Some("23514") => match database.message() {
            "PROJECT_STRUCTURE_FROZEN" => Some(structure_frozen()),
            _ => None,
        },
        _ => None,
    };
    code.map_or_else(|| error.into(), Into::into)
}

type ProjectRow = (String, String, Option<String>, Option<String>, String, String, i64, bool, bool);
const PROJECT: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.owner_user_id,\
    p.lifecycle_status,p.version,\
    iam_same_person(p.owner_user_id,$3,p.company_id) AS is_owner,\
    EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id \
      AND iam_same_person(m.user_id,$3,p.company_id)) AS is_member \
    FROM staff_projects p WHERE p.id=$1 AND p.company_id=$2 \
      AND (iam_same_person(p.owner_user_id,$3,p.company_id) \
        OR EXISTS(SELECT 1 FROM staff_project_members m2 WHERE m2.project_id=p.id \
          AND iam_same_person(m2.user_id,$3,p.company_id))) FOR UPDATE OF p";

fn project_row(
    (id, company_id, branch_id, department_id, owner_user_id, lifecycle_status, version, is_owner, is_member): ProjectRow,
) -> ProjectCommandProject {
    ProjectCommandProject {
        id,
        company_id,
        branch_id,
        department_id,
        owner_user_id,
        lifecycle_status,
        version,
        is_owner,
        is_member,
    }
}

async fn read_project(
    connection: &mut PgConnection,
    company: &CompanyId,
    project_id: &str,
    user_id: &str,
) -> Result<Option<ProjectCommandProject>, Error> {
    sqlx::query_as::<_, ProjectRow>(PROJECT)
        .bind(project_id)
        .bind(company.as_str())
        .bind(user_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(sql_error)
        .map(|row| row.map(project_row))
}

async fn write_receipt(
    connection: &mut PgConnection,
    plan: &SaveProjectPlan,
    receipt: &SaveProjectReceipt,
) -> Result<(), Error> {
    let result_json = serde_json::to_string(receipt).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "PROJECT_COMMAND_RECEIPT_INVALID",
            "The Project command receipt could not be retained.",
        )
    })?;
    sqlx::query(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
         result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(plan.key.as_str())
    .bind(&plan.operation)
    .bind(&plan.actor_id)
    .bind(&plan.request_fingerprint)
    .bind(&plan.project_id)
    .bind(result_json)
    .bind(plan.now)
    .bind(&plan.authority_json)
    .execute(&mut *connection)
    .await
    .map_err(sql_error)?;
    Ok(())
}

async fn write_audit_outbox(
    connection: &mut PgConnection,
    plan: &SaveProjectPlan,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
         record_id,record_version,purpose,created_at,authority_json) \
         VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'project',$5,$6,$4,$7,$8)",
    )
    .bind(plan.key.as_str())
    .bind(&plan.actor_id)
    .bind(&plan.assignment_id)
    .bind(&plan.operation)
    .bind(&plan.project_id)
    .bind(plan.version)
    .bind(plan.now)
    .bind(&plan.authority_json)
    .execute(&mut *connection)
    .await?;
    let payload = serde_json::json!({
        "authority": serde_json::from_str::<serde_json::Value>(&plan.authority_json)
            .unwrap_or(serde_json::Value::Null),
        "id": plan.project_id,
        "companyId": plan.company_id.as_str(),
        "version": plan.version,
    });
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at) \
         VALUES(gen_random_uuid()::text,$1,$2,'project',$3,$4,$5,$6)",
    )
    .bind(plan.key.as_str())
    .bind(&plan.operation)
    .bind(&plan.project_id)
    .bind(plan.version)
    .bind(payload.to_string())
    .bind(plan.now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

pub struct PgProjectSaveCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgProjectSaveCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn stored_receipt(
        &mut self,
        key: &str,
    ) -> Result<Option<StoredProjectReceipt>, Error> {
        serializable_checked(&mut *self.connection).await?;
        sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT operation,record_id,request_hash,result_json FROM command_receipts \
             WHERE command_id=$1",
        )
        .bind(key)
        .fetch_optional(&mut *self.connection)
        .await
        .map(|row| {
            row.map(
                |(operation, record_id, request_fingerprint, result_json)| StoredProjectReceipt {
                    operation,
                    record_id,
                    request_fingerprint,
                    result_json,
                },
            )
        })
        .map_err(Into::into)
    }

    pub async fn project(
        &mut self,
        company: &CompanyId,
        project_id: &str,
        user_id: &str,
    ) -> Result<Option<ProjectCommandProject>, Error> {
        serializable_checked(&mut *self.connection).await?;
        read_project(&mut *self.connection, company, project_id, user_id).await
    }

    pub async fn save(&mut self, plan: SaveProjectPlan) -> Result<SaveProjectReceipt, Error> {
        serializable_checked(&mut *self.connection).await?;
        // Retained `standaloneProjectSourceReadLocks` held the same two owner
        // tables while the project brief and membership were replaced.
        sqlx::query(
            "LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE",
        )
        .execute(&mut *self.connection)
        .await?;
        let current = read_project(
            &mut *self.connection,
            &plan.company_id,
            &plan.project_id,
            &plan.actor_id,
        )
        .await?;
        if plan.project_version == 0 {
            if current.is_some() {
                return Err(source_changed().into());
            }
            sqlx::query(
                "INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,\
                 owner_user_id,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,1,$8,$8)",
            )
            .bind(&plan.project_id)
            .bind(plan.company_id.as_str())
            .bind(&plan.branch_id)
            .bind(&plan.department_id)
            .bind(&plan.title)
            .bind(&plan.description)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
        } else {
            let current = current.ok_or_else(missing_project)?;
            if current.version != plan.project_version
                || current.lifecycle_status != "active"
                || !current.is_owner
            {
                return Err(source_changed().into());
            }
            let updated = sqlx::query(
                "UPDATE staff_projects SET title=$1,description=$2,version=version+1,updated_at=$3 \
                 WHERE id=$4 AND company_id=$5 AND version=$6 AND lifecycle_status='active' \
                   AND iam_same_person(owner_user_id,$7,company_id)",
            )
            .bind(&plan.title)
            .bind(&plan.description)
            .bind(plan.now)
            .bind(&plan.project_id)
            .bind(plan.company_id.as_str())
            .bind(plan.project_version)
            .bind(&plan.actor_id)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
            if updated.rows_affected() != 1 {
                return Err(version_conflict().into());
            }
        }
        // Retained membership replacement: the complete current set replaces the
        // prior set, and the stored list is de-duplicated exactly.
        sqlx::query("DELETE FROM staff_project_members WHERE project_id=$1")
            .bind(&plan.project_id)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
        for user in &plan.member_user_ids {
            sqlx::query(
                "INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) \
                 VALUES($1,$2,$3,$4)",
            )
            .bind(&plan.project_id)
            .bind(user)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(sql_error)?;
        }
        let receipt = SaveProjectReceipt {
            id: plan.project_id.clone(),
            version: plan.version,
        };
        write_receipt(&mut *self.connection, &plan, &receipt).await?;
        write_audit_outbox(&mut *self.connection, &plan).await?;
        Ok(receipt)
    }
}

impl SaveProjectRepository for PgProjectSaveCommands<'_> {
    type Error = Error;

    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> ProjectSaveFuture<'a, Option<StoredProjectReceipt>, Self::Error> {
        Box::pin(async move { PgProjectSaveCommands::stored_receipt(self, key).await })
    }

    fn project<'a>(
        &'a mut self,
        company: &'a CompanyId,
        project_id: &'a str,
        user_id: &'a str,
    ) -> ProjectSaveFuture<'a, Option<ProjectCommandProject>, Self::Error> {
        Box::pin(async move {
            PgProjectSaveCommands::project(self, company, project_id, user_id).await
        })
    }

    fn save<'a>(
        &'a mut self,
        plan: SaveProjectPlan,
    ) -> ProjectSaveFuture<'a, SaveProjectReceipt, Self::Error> {
        Box::pin(async move { PgProjectSaveCommands::save(self, plan).await })
    }
}
