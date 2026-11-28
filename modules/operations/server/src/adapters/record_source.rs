//! Read-only owner source facts for Operations staff tasks, fenced to the
//! caller's serializable unit.
//!
//! This module reads current task facts and participant rules. It never opens
//! an independent transaction or fabricates a missing or foreign source.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::record_source_contract::SourceRecord;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SOURCE_RECORD_CHANGED",
        "The owning source changed.",
    )
    .into()
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("RECORD_SOURCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<(SourceRecord, String)>, Error> {
    let row = sqlx::query(SQL)
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(c)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let raw: Option<String> = row.try_get("payload")?;
    let source = serde_json::from_str(&raw.ok_or_else(|| {
        BusinessError::new(
            FailureKind::Unavailable,
            "RECORD_SOURCE_LIMIT",
            "The source exceeds its bounded read.",
        )
    })?)
    .map_err(|_| changed())?;
    Ok(Some((source, row.try_get("digest")?)))
}

pub struct CurrentRecordSource {
    company: CompanyId,
    id: String,
    unit: Unit,
    image: (SourceRecord, String),
}

impl CurrentRecordSource {
    pub fn data(&self) -> &SourceRecord {
        &self.image.0
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.id).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}

pub async fn read_record_source(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentRecordSource>, Error> {
    let unit = unit(c).await?;
    Ok(capture(c, company, id)
        .await?
        .map(|image| CurrentRecordSource {
            company: company.clone(),
            id: id.into(),
            unit,
            image,
        }))
}

pub async fn lock_record_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("RECORD_SOURCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE staff_tasks IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// Owner facts for a Social task reference. Callers still apply the selected
/// session, purchase and originating conversation checks before publishing.
#[derive(Debug)]
pub struct SocialTaskSource {
    pub id: String,
    pub title: String,
    pub state: String,
    pub version: i64,
    pub project_id: Option<String>,
    pub template_id: Option<String>,
    pub origin_post_id: Option<String>,
    pub preview_participant: bool,
    pub handover_participant: bool,
}

/// Handover retains the original any-active-delegate rule. The Task preview
/// uses the most recent active delegation and returns its Social origin for
/// the separate current-post check.
pub async fn read_social_task_source(
    connection: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    user_id: &str,
    now: i64,
) -> Result<Option<SocialTaskSource>, Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("SOCIAL_TASK_TRANSACTION_REQUIRED").into());
    }
    let row = sqlx::query(SOCIAL_TASK)
        .bind(id)
        .bind(company.as_str())
        .bind(user_id)
        .bind(now)
        .fetch_optional(connection)
        .await?;
    row.map(|row| {
        Ok(SocialTaskSource {
            id: row.try_get("id")?,
            title: row.try_get("title")?,
            state: row.try_get("state")?,
            version: row.try_get("version")?,
            project_id: row.try_get("project_id")?,
            template_id: row.try_get("template_id")?,
            origin_post_id: row.try_get("post_id")?,
            preview_participant: row.try_get("preview_participant")?,
            handover_participant: row.try_get("handover_participant")?,
        })
    })
    .transpose()
}

const SOCIAL_TASK: &str = r#"
WITH task_source AS (
    SELECT task.id,task.title,task.state,task.version,task.project_id,task.template_id,
           origin.post_id,task.company_id,
           COALESCE(
              iam_same_person(task.creator_user_id,$3,task.company_id)
           OR iam_same_person(task.assignee_user_id,$3,task.company_id)
           OR iam_same_person(project.owner_user_id,$3,task.company_id)
           OR EXISTS(SELECT 1 FROM staff_project_members member
                     WHERE member.project_id=project.id AND member.user_id=$3)
           OR EXISTS(SELECT 1 FROM erpx_json_each(task.start_approvers_json) WHERE value=$3)
           OR EXISTS(SELECT 1 FROM erpx_json_each(task.completion_approvers_json) WHERE value=$3)
           OR EXISTS(
               SELECT 1 FROM approval_instances instance
               JOIN approval_steps step ON step.instance_id=instance.id
               WHERE instance.record_type='task' AND instance.record_id=task.id
                 AND instance.company_id=task.company_id AND EXISTS(
                     SELECT 1 FROM (
                         SELECT step.approver_user_id AS user_id
                         UNION SELECT value FROM erpx_json_each(COALESCE(step.approver_pool_json,'[]'))
                     ) participant WHERE iam_same_person(participant.user_id,$3,instance.company_id)
                 )
           ),false) AS common_participant
    FROM staff_tasks task
    LEFT JOIN staff_projects project ON project.id=task.project_id AND project.company_id=task.company_id
    LEFT JOIN social_task_origins origin ON origin.task_id=task.id
    WHERE task.id=$1 AND task.company_id=$2
), delegations AS (
    SELECT delegation.delegate_user_id FROM staff_task_delegations delegation
    WHERE delegation.task_id=$1 AND delegation.company_id=$2 AND delegation.status='active'
      AND delegation.starts_at<=$4 AND delegation.expires_at>$4
    ORDER BY delegation.created_at DESC,delegation.id DESC
)
SELECT task_source.*,
       (common_participant OR COALESCE(
           iam_same_person((SELECT delegate_user_id FROM delegations LIMIT 1),$3,company_id),false
       )) AS preview_participant,
       (post_id IS NULL AND (common_participant OR EXISTS(
           SELECT 1 FROM delegations WHERE iam_same_person(delegate_user_id,$3,company_id)
       ))) AS handover_participant
FROM task_source
"#;

const SQL: &str = "WITH s AS MATERIALIZED(SELECT * FROM staff_tasks WHERE id=$1 AND company_id=$2 FOR SHARE),payload AS(SELECT jsonb_build_object('kind','staff_task','id',s.id,'company_id',s.company_id,'branch_id',s.branch_id,'department_id',s.department_id,'version',s.version,'title',s.title,'status',s.state)::text value,to_jsonb(s)::text image FROM s) SELECT CASE WHEN octet_length(value)<=2097152 AND octet_length(image)<=2097152 THEN value END payload,encode(sha256(convert_to(image,'UTF8')),'hex') digest FROM payload";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn staff_task_read_uses_only_the_owner_table_and_portable_shape() {
        assert!(SQL.contains("FROM staff_tasks"));
        assert!(SQL.contains("'staff_task'"));
        assert!(SQL.contains("'status',s.state"));
        for key in [
            "'id'",
            "'company_id'",
            "'branch_id'",
            "'department_id'",
            "'version'",
            "'title'",
            "'status'",
        ] {
            assert!(SQL.contains(key), "missing payload key {key}");
        }
        assert!(SQL.contains("octet_length(value)<=2097152"));
        assert!(SQL.contains("octet_length(image)<=2097152"));
        assert!(SQL.contains("sha256"));
    }

    #[test]
    fn captured_payload_deserializes_to_the_portable_record() {
        let payload = json!({
            "kind": "staff_task",
            "id": "task-1",
            "company_id": "company",
            "branch_id": "branch",
            "department_id": null,
            "version": 7,
            "title": "Synthetic task",
            "status": "in_progress",
        });
        let source: SourceRecord = serde_json::from_value(payload).unwrap();
        assert_eq!(
            source.kind,
            erpx_platform_shared_server::public::record_source_contract::SourceKind::StaffTask
        );
        assert_eq!(source.version, 7);
    }
}
