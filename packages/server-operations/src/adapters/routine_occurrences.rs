//! PostgreSQL scheduled routine occurrence generator.
//!
//! One bounded pass selects active due schedules in cursor order, then runs
//! one caller-owned serializable unit per occurrence. The unit owns the task,
//! cursor, journal, execution step and deferred authorization/purchase checks;
//! the runtime retries the complete unit on serialization/deadlock failures.
//! Cross-owner authority, purchase and route facts arrive through composition.
use crate::domain::routine_commands::{RoutineScheduleRow, RoutineTemplateRow};
use crate::domain::routine_occurrences::{
    OccurrenceOutcome, OccurrencePass, OccurrencePolicy, OccurrenceSource,
    OccurrenceSourceFacts, RoutineExecutionAuthorization, RoutineExecutionStep, StoredRoutineAuthority,
    authorization_required, source_changed,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
use std::pin::Pin;

type Error = PgTransactionError<BusinessError>;

pub type RoutineOccurrenceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'a>>;

/// One occurrence unit supplied by composition. The callback owns authorization,
/// purchase fences and the native task write; this adapter owns the bounded
/// pass, cursor/error handling and retry.
pub trait RoutineOccurrenceUnit: Send {
    fn run<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        schedule: &'a RoutineScheduleRow,
        at: i64,
        now: i64,
    ) -> RoutineOccurrenceFuture<'a, Option<OccurrenceOutcome>>;
}

async fn require_serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(
            erpx_server_runtime::public::postgres::PersistenceError::new(
                "ROUTINE_OCCURRENCE_SERIALIZABLE_REQUIRED",
            )
            .into(),
        );
    }
    Ok(())
}

fn decode<T: serde::de::DeserializeOwned>(image: &str) -> Result<T, Error> {
    serde_json::from_str(image).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "ROUTINE_OCCURRENCE_READ_INVALID",
            "The scheduled routine source could not be read.",
        )
        .into()
    })
}

/// One bounded due-schedule read. `policy.scope_sql` is composition-owned and
/// already carries the queued authorization, purchase and production filters.
pub async fn due_schedules(
    connection: &mut PgConnection,
    now: i64,
    limit: i64,
    policy: &OccurrencePolicy,
) -> Result<Vec<RoutineScheduleRow>, Error> {
    let limit = limit.clamp(0, 20);
    if limit == 0 {
        return Ok(Vec::new());
    }
    let scope = policy
        .scope_sql
        .as_deref()
        .map(|sql| format!(" AND ({sql})"))
        .unwrap_or_default();
    let production = if policy.production {
        " AND EXISTS(SELECT 1 FROM company_feature_rollouts f \
         WHERE f.company_id=s.company_id AND f.module_id='operations' \
         AND f.rollout_state IN ('pilot','available'))"
    } else {
        ""
    };
    let sql = format!(
        "SELECT to_jsonb(s)::text FROM routine_schedules s \
         WHERE s.state='active' AND s.next_occurrence_at<=$1{production}{scope} \
         ORDER BY s.next_occurrence_at,s.id LIMIT $2"
    );
    let images = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .bind(now)
        .bind(limit)
        .fetch_all(&mut *connection)
        .await?;
    images.iter().map(|image| decode(image)).collect()
}

pub async fn source(
    connection: &mut PgConnection,
    company_id: &str,
    schedule_id: &str,
    at: i64,
) -> Result<Option<OccurrenceSource>, Error> {
    let schedule: Option<String> = sqlx::query_scalar(
        "SELECT to_jsonb(s)::text FROM routine_schedules s WHERE s.id=$1 AND s.company_id=$2",
    )
    .bind(schedule_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(schedule) = schedule else {
        return Ok(None);
    };
    let schedule: RoutineScheduleRow = decode(&schedule)?;
    let pause_ended_at: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT ended_at FROM routine_schedule_pauses WHERE schedule_id=$1 AND started_at<=$2 \
         AND (ended_at IS NULL OR ended_at>$2) ORDER BY started_at DESC LIMIT 1",
    )
    .bind(schedule_id)
    .bind(at)
    .fetch_optional(&mut *connection)
    .await?;
    let template: Option<String> = sqlx::query_scalar(
        "SELECT to_jsonb(t)::text FROM routine_templates t WHERE t.id=$1 AND t.company_id=$2",
    )
    .bind(&schedule.template_id)
    .bind(company_id)
    .fetch_optional(&mut *connection)
    .await?;
    let template = template.map(|image| decode(&image)).transpose()?;
    let prior: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM staff_tasks WHERE schedule_id=$1 AND occurrence_at=$2)",
    )
    .bind(schedule_id)
    .bind(at)
    .fetch_one(&mut *connection)
    .await?;
    Ok(Some(OccurrenceSource {
        schedule,
        template,
        pause_exists: pause_ended_at.is_some(),
        pause_ended_at: pause_ended_at.flatten(),
        prior,
    }))
}

pub async fn source_facts(
    connection: &mut PgConnection,
    company_id: &str,
    template: &RoutineTemplateRow,
    start_blocked: Option<String>,
    completion_blocked: Option<String>,
    facts: &OccurrenceSourceFacts,
) -> Result<OccurrenceSourceFacts, Error> {
    let project_available = match template.project_id.as_deref() {
        Some(project_id) => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM staff_projects WHERE id=$1 AND company_id=$2)",
        )
        .bind(project_id)
        .bind(company_id)
        .fetch_one(&mut *connection)
        .await?,
        None => true,
    };
    Ok(OccurrenceSourceFacts {
        project_available,
        start_blocked,
        completion_blocked,
        ..facts.clone()
    })
}

pub async fn authorization(
    connection: &mut PgConnection,
    schedule_id: &str,
    schedule_version: i64,
) -> Result<Option<RoutineExecutionAuthorization>, Error> {
    let row = sqlx::query(
        "SELECT command_id,schedule_id,template_id,company_id,actor_id,assignment_id,\
         authority_json,source_hash,template_version,schedule_version,initial_occurrence_at,\
         created_at FROM work_routine_execution_authorizations \
         WHERE schedule_id=$1 AND schedule_version=$2 \
         ORDER BY created_at DESC,command_id DESC LIMIT 1",
    )
    .bind(schedule_id)
    .bind(schedule_version)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| RoutineExecutionAuthorization {
        command_id: row.get("command_id"),
        schedule_id: row.get("schedule_id"),
        template_id: row.get("template_id"),
        company_id: row.get("company_id"),
        actor_id: row.get("actor_id"),
        assignment_id: row.get("assignment_id"),
        authority_json: row.get("authority_json"),
        source_hash: row.get("source_hash"),
        template_version: row.get("template_version"),
        schedule_version: row.get("schedule_version"),
        initial_occurrence_at: row.get("initial_occurrence_at"),
        created_at: row.get("created_at"),
    }))
}

/// Repeated retained source check: the current template/schedule configuration
/// image must hash to the captured authorization `source_hash`, and the captured
/// template/company/version/actor identities must still agree.
pub async fn require_authorization_source(
    connection: &mut PgConnection,
    authorization: &RoutineExecutionAuthorization,
    template: &RoutineTemplateRow,
    schedule: &RoutineScheduleRow,
    actor: &StoredRoutineAuthority,
) -> Result<(), Error> {
    if authorization.template_id != template.id
        || authorization.company_id != schedule.company_id
        || authorization.template_version != template.version
        || authorization.schedule_id != schedule.id
        || authorization.schedule_version != schedule.version
        || actor.company_id != schedule.company_id
        || actor.user_id != authorization.actor_id
        || actor.assignment_id != authorization.assignment_id
    {
        return Err(source_changed().into());
    }
    let matches: bool = sqlx::query_scalar(
        "SELECT encode(sha256(convert_to((SELECT jsonb_build_object(\
         'template',to_jsonb(t),\
         'schedule',to_jsonb(s)-ARRAY['next_occurrence_at','last_generated_at','last_error',\
         'updated_at','paused_at','state'])::text \
         FROM routine_templates t JOIN routine_schedules s \
         ON s.template_id=t.id AND s.company_id=t.company_id \
         WHERE t.id=$1 AND s.id=$2 AND t.company_id=$3),'UTF8')),'hex')=$4",
    )
    .bind(&template.id)
    .bind(&schedule.id)
    .bind(&schedule.company_id)
    .bind(&authorization.source_hash)
    .fetch_one(&mut *connection)
    .await?;
    if !matches {
        return Err(source_changed().into());
    }
    Ok(())
}

pub async fn steps(
    connection: &mut PgConnection,
    authorization_command_id: &str,
) -> Result<Vec<RoutineExecutionStep>, Error> {
    let rows = sqlx::query(
        "SELECT command_id,occurrence_at,next_occurrence_at,task_id,generated,source_hash \
         FROM work_routine_execution_steps WHERE authorization_command_id=$1 \
         ORDER BY occurrence_at",
    )
    .bind(authorization_command_id)
    .fetch_all(&mut *connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| RoutineExecutionStep {
            command_id: row.get("command_id"),
            occurrence_at: row.get("occurrence_at"),
            next_occurrence_at: row.get("next_occurrence_at"),
            task_id: row.get("task_id"),
            generated: row.get("generated"),
            source_hash: row.get("source_hash"),
        })
        .collect())
}

/// Retained contiguous lineage proof, including the generated task audit and
/// outbox journals. The supplied cursor is the occurrence about to run.
pub async fn require_lineage(
    connection: &mut PgConnection,
    authorization: &RoutineExecutionAuthorization,
    cursor: Option<i64>,
) -> Result<(), Error> {
    let mut expected = authorization.initial_occurrence_at;
    for step in steps(connection, &authorization.command_id).await? {
        if step.occurrence_at != expected.unwrap_or_default() || step.source_hash != authorization.source_hash {
            return Err(source_changed().into());
        }
        if step.generated {
            let valid: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM staff_tasks task \
                 JOIN audit_events audit ON audit.command_id=$1 AND audit.record_id=task.id \
                 AND audit.record_version=1 AND audit.action='routine.occurrence' \
                 AND audit.record_type='staff_task' \
                 JOIN outbox_events outbox ON outbox.command_id=audit.command_id \
                 AND outbox.aggregate_id=task.id AND outbox.aggregate_version=1 \
                 AND outbox.event_type='task.generated' AND outbox.aggregate_type='staff_task' \
                 WHERE task.id=$2 AND task.company_id=$3 AND task.schedule_id=$4 \
                 AND task.occurrence_at=$5 AND task.template_id=$6 AND task.template_version=$7)",
            )
            .bind(&step.command_id)
            .bind(step.task_id.as_deref())
            .bind(&authorization.company_id)
            .bind(&authorization.schedule_id)
            .bind(step.occurrence_at)
            .bind(&authorization.template_id)
            .bind(authorization.template_version)
            .fetch_one(&mut *connection)
            .await?;
            if !valid {
                return Err(source_changed().into());
            }
        }
        expected = step.next_occurrence_at;
    }
    if expected != cursor {
        return Err(source_changed().into());
    }
    Ok(())
}

fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "WORK_ROUTINE_AUTHORITY_CHANGED",
        "The original routine assignment or Company role is no longer current.",
    )
}

/// Reproduce the retained `current()` IAM/assignment/staff checks for the
/// captured authorization actor. Optional IAM/session witnesses are checked
/// when the producer captured them; the native minimal context checks the
/// current assignment, scope, role and active staff.
pub async fn require_current_authority(
    connection: &mut PgConnection,
    authorization: &RoutineExecutionAuthorization,
    actor: &StoredRoutineAuthority,
    now: i64,
) -> Result<(), Error> {
    if actor.company_id != authorization.company_id
        || actor.user_id != authorization.actor_id
        || actor.assignment_id != authorization.assignment_id
        || actor.role.is_empty()
    {
        return Err(authority_changed().into());
    }
    let assignment_version = authorization.assignment_version(actor);
    let role_revision = actor.iam.as_ref().and_then(|iam| iam.role_revision);
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments g \
         WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
         AND ($5::integer IS NULL OR g.version=$5) \
         AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
         AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$6 \
           OR g.scope_kind='department' AND g.scope_node_id=$7) \
         AND EXISTS(SELECT 1 FROM iam_roles role WHERE role.id=g.role \
           AND role.company_id IN ('*',g.company_id) AND role.status='active' \
           AND ($8::integer IS NULL OR role.published_revision=$8)))",
    )
    .bind(&actor.assignment_id)
    .bind(&actor.user_id)
    .bind(&actor.role)
    .bind(&actor.company_id)
    .bind(assignment_version)
    .bind(actor.branch_id.as_deref())
    .bind(actor.department_id.as_deref())
    .bind(role_revision)
    .fetch_one(&mut *connection)
    .await?;
    if !allowed {
        return Err(authority_changed().into());
    }
    if let Some(session) = actor.authority.as_ref().and_then(|value| value.session_id.as_deref()) {
        let revoked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions \
             WHERE session_id=$1 AND company_id=$2 AND user_id=$3)",
        )
        .bind(session)
        .bind(&actor.company_id)
        .bind(&actor.user_id)
        .fetch_one(&mut *connection)
        .await?;
        if revoked {
            return Err(authority_changed().into());
        }
    }
    if let Some(authority) = actor.authority.as_ref() {
        if let (Some(session), Some(revision), Some(version)) = (
            authority.session_id.as_deref(),
            authority.revision,
            authority.assignment_version,
        ) {
            let current: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM active_work_contexts c \
                 WHERE c.session_id=$1 AND c.company_id=$2 AND c.user_id=$3 \
                 AND c.assignment_id=$4 AND c.revision=$5 AND c.assignment_version=$6 \
                 AND c.expires_at>$7)",
            )
            .bind(session)
            .bind(&actor.company_id)
            .bind(&actor.user_id)
            .bind(&actor.assignment_id)
            .bind(revision)
            .bind(version)
            .bind(now)
            .fetch_one(&mut *connection)
            .await?;
            if !current {
                return Err(authority_changed().into());
            }
        }
    }
    if let Some(iam) = actor.iam.as_ref() {
        if let Some(version) = iam.identity_version {
            let current: bool = sqlx::query_scalar(
                "SELECT COALESCE((SELECT version FROM iam_members \
                 WHERE company_id=$1 AND user_id=$2),0)=$3",
            )
            .bind(&actor.company_id)
            .bind(&actor.user_id)
            .bind(version)
            .fetch_one(&mut *connection)
            .await?;
            if !current {
                return Err(authority_changed().into());
            }
        }
        if let Some(until) = iam.emergency_until
            && until <= now
        {
            return Err(authority_changed().into());
        }
        if let Some(version) = iam.hr_appointment_version {
            let current: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM authority_seats s \
                 JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id \
                 JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 WHERE s.company_id=$1 AND s.kind='hr' AND s.scope_id=$1 AND s.version=$2 \
                 AND s.effective_at<=$3 AND p.status='active' AND p.merged_into_id IS NULL \
                 AND e.status='active' AND iam_same_person(p.user_id,$4,s.company_id))",
            )
            .bind(&actor.company_id)
            .bind(version)
            .bind(now)
            .bind(&actor.user_id)
            .fetch_one(&mut *connection)
            .await?;
            if !current {
                return Err(authority_changed().into());
            }
        }
    }
    let staff: bool = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM hr_people p \
         JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
         WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL \
         AND iam_same_person(p.user_id,$2,p.company_id) AND e.status='active' \
         AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') \
         AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1",
    )
    .bind(&actor.company_id)
    .bind(&actor.user_id)
    .fetch_one(&mut *connection)
    .await?;
    if !staff {
        return Err(authority_changed().into());
    }
    Ok(())
}

pub async fn production_enabled(
    connection: &mut PgConnection,
    company_id: &str,
) -> Result<bool, Error> {
    let enabled: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM company_feature_rollouts f \
         WHERE f.company_id=$1 AND f.module_id='operations' \
         AND f.rollout_state IN ('pilot','available'))",
    )
    .bind(company_id)
    .fetch_one(&mut *connection)
    .await?;
    if enabled {
        sqlx::query(
            "SELECT 1 FROM company_feature_rollouts \
             WHERE company_id=$1 AND module_id='operations' FOR SHARE",
        )
        .bind(company_id)
        .execute(&mut *connection)
        .await?;
    }
    Ok(enabled)
}

pub fn production_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FEATURE_COMING_SOON",
        "This feature is outside the approved release scopes",
    )
}

/// Retained locked source order before the first owner snapshot.
pub async fn lock_occurrence_sources(
    connection: &mut PgConnection,
    company_id: &str,
) -> Result<(), Error> {
    require_serializable(connection).await?;
    crate::adapters::routine_commands::lock_routine_sources(connection).await?;
    sqlx::query(
        "LOCK TABLE approval_instances,approval_steps,approval_decisions,staff_projects,\
         staff_project_members IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "LOCK TABLE staff_projects,staff_project_members IN SHARE MODE",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,\
         iam_connections,iam_revoked_sessions,iam_emergency_sessions,active_work_contexts,\
         iam_access_subjects,iam_access_invitations,um_attributes,um_values,um_groups,\
         um_memberships,um_policies,um_attachments,um_candidates,\
         iam_directory_source_selections,iam_directory_observation_sources IN SHARE MODE",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "LOCK TABLE organization_nodes,authority_seats,organization_positions IN SHARE MODE",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "LOCK TABLE hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,\
         people_lifecycle_grants,people_package_revisions,people_applied_packages,\
         people_provision_operations,people_provision_grants IN SHARE MODE",
    )
    .execute(&mut *connection)
    .await?;
    sqlx::query("LOCK TABLE company_feature_rollouts IN SHARE MODE")
        .execute(&mut *connection)
        .await?;
    let _ = company_id;
    Ok(())
}

pub async fn occurrence_image(
    connection: &mut PgConnection,
    company_id: &str,
    template_id: &str,
    schedule_id: &str,
    task_id: &str,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(\
         'template',(SELECT to_jsonb(t) FROM routine_templates t WHERE t.id=$1 AND t.company_id=$2),\
         'schedule',(SELECT to_jsonb(s) FROM routine_schedules s WHERE s.id=$3 AND s.company_id=$2),\
         'pauses',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM routine_schedule_pauses p WHERE p.schedule_id=$3),\
         'task',(SELECT to_jsonb(t) FROM staff_tasks t WHERE t.id=$4 AND t.company_id=$2),\
         'approval',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(i),\
         'steps',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM approval_steps s WHERE s.instance_id=i.id)) ORDER BY i.id)\
         FROM approval_instances i WHERE i.record_id=$4 AND i.company_id=$2),\
         'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM audit_events a \
         WHERE a.record_id=$4 AND a.action='routine.occurrence' AND a.record_type='staff_task'),\
         'outbox',(SELECT jsonb_agg(to_jsonb(o)-ARRAY['published_at','claim_until','attempts'] ORDER BY o.id) \
         FROM outbox_events o WHERE o.aggregate_id=$4 AND o.event_type='task.generated' \
         AND o.aggregate_type='staff_task'))::text",
    )
    .bind(template_id)
    .bind(company_id)
    .bind(schedule_id)
    .bind(task_id)
    .fetch_one(&mut *connection)
    .await?)
}

pub async fn effects_hash(
    connection: &mut PgConnection,
    image: &str,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar(
        "SELECT encode(sha256(convert_to($1,'UTF8')),'hex')",
    )
    .bind(image)
    .fetch_one(&mut *connection)
    .await?)
}

/// Stage the occurrence cursor, generated task journal and immutable execution
/// step in the caller's owner unit. A generated task keeps the retained
/// `routine.occurrence` audit and `task.generated` outbox rows.
#[allow(clippy::too_many_arguments)]
pub async fn stage_occurrence(
    connection: &mut PgConnection,
    authorization: Option<&RoutineExecutionAuthorization>,
    schedule_id: &str,
    company_id: &str,
    schedule_version: i64,
    template_id: &str,
    at: i64,
    next: Option<i64>,
    generated: bool,
    task_id: &str,
    task_state: &str,
    branch_id: Option<&str>,
    department_id: Option<&str>,
    now: i64,
    assertion_keys: &[String],
) -> Result<Option<String>, Error> {
    let updated = sqlx::query(
        "UPDATE routine_schedules SET next_occurrence_at=$1,\
         state=CASE WHEN $2::bigint IS NULL THEN 'ended' ELSE state END,\
         last_generated_at=CASE WHEN $3 THEN $4 ELSE last_generated_at END,\
         last_error=NULL,updated_at=$4 \
         WHERE id=$5 AND version=$6 AND state='active' AND next_occurrence_at=$7",
    )
    .bind(next)
    .bind(next)
    .bind(generated)
    .bind(now)
    .bind(schedule_id)
    .bind(schedule_version)
    .bind(at)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(source_changed().into());
    }
    if generated {
        let key = format!("occurrence-{schedule_id}-{at}");
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at) \
             VALUES($1,$1,'system','scheduler','routine.occurrence','staff_task',$2,1,\
             'Generate scheduled occurrence',$3)",
        )
        .bind(&key)
        .bind(task_id)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        let payload = serde_json::json!({
            "id": task_id,
            "state": task_state,
            "version": 1,
            "companyId": company_id,
            "branchId": branch_id,
            "departmentId": department_id,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$1,'task.generated','staff_task',$2,1,$3,$4)",
        )
        .bind(&key)
        .bind(task_id)
        .bind(payload)
        .bind(now)
        .execute(&mut *connection)
        .await?;
    }
    let completed = if authorization.is_some() {
        occurrence_image(connection, company_id, template_id, schedule_id, task_id).await?
    } else {
        None
    };
    if let Some(authorization) = authorization {
        let image = completed.as_deref().ok_or_else(source_changed)?;
        let hash = effects_hash(connection, image).await?;
        sqlx::query(
            "INSERT INTO work_routine_execution_steps(command_id,authorization_command_id,\
             schedule_id,company_id,occurrence_at,next_occurrence_at,task_id,generated,\
             source_hash,effects_hash,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(format!("occurrence-{schedule_id}-{at}"))
        .bind(&authorization.command_id)
        .bind(schedule_id)
        .bind(company_id)
        .bind(at)
        .bind(next)
        .bind(generated.then_some(task_id))
        .bind(generated)
        .bind(&authorization.source_hash)
        .bind(hash)
        .bind(now)
        .execute(&mut *connection)
        .await?;
    }
    if !assertion_keys.is_empty() {
        sqlx::query("DELETE FROM command_assertions WHERE command_id=ANY($1)")
            .bind(assertion_keys)
            .execute(&mut *connection)
            .await?;
    }
    Ok(completed)
}

pub async fn record_error(
    connection: &mut PgConnection,
    schedule_id: &str,
    version: i64,
    message: &str,
    now: i64,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE routine_schedules SET last_error=$1,updated_at=$2 WHERE id=$3 AND version=$4",
    )
    .bind(message.chars().take(500).collect::<String>())
    .bind(now)
    .bind(schedule_id)
    .bind(version)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct LatestScheduleCursor {
    pub next_occurrence_at: Option<i64>,
    pub version: i64,
    pub state: String,
}

pub async fn latest(
    connection: &mut PgConnection,
    schedule_id: &str,
) -> Result<Option<LatestScheduleCursor>, Error> {
    let row = sqlx::query(
        "SELECT next_occurrence_at,version,state FROM routine_schedules WHERE id=$1",
    )
    .bind(schedule_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| LatestScheduleCursor {
        next_occurrence_at: row.get("next_occurrence_at"),
        version: row.get("version"),
        state: row.get("state"),
    }))
}

pub async fn lag(connection: &mut PgConnection, now: i64) -> Result<Option<i64>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT MIN(next_occurrence_at) FROM routine_schedules \
         WHERE state='active' AND next_occurrence_at<=$1",
    )
    .bind(now)
    .fetch_one(&mut *connection)
    .await?)
}

fn error_message(error: &Error) -> String {
    match error {
        PgTransactionError::Business(error) => error.message.to_owned(),
        PgTransactionError::Persistence(error) => error.code.to_owned(),
    }
}

/// One occurrence attempt under a fresh serializable transaction. A
/// serialization/deadlock failure retries the complete unit (four attempts,
/// matching `PgUnitOfWork::run`); a business refusal is returned unchanged.
async fn run_occurrence_unit<U: RoutineOccurrenceUnit>(
    pool: &DatabasePool,
    unit: &mut U,
    schedule: &RoutineScheduleRow,
    at: i64,
    now: i64,
) -> Result<Option<OccurrenceOutcome>, Error> {
    let mut last = None;
    for attempt in 0..4 {
        let mut transaction = pool.raw().begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *transaction)
            .await?;
        match unit.run(&mut transaction, schedule, at, now).await {
            Ok(value) => {
                transaction.commit().await?;
                return Ok(value);
            }
            Err(error) => {
                let retry = matches!(
                    &error,
                    PgTransactionError::Persistence(error) if error.retryable()
                ) && attempt + 1 < 4;
                transaction.rollback().await?;
                if !retry {
                    return Err(error);
                }
                last = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(10 * (1 << attempt))).await;
            }
        }
    }
    Err(last.expect("a failed retry is retained"))
}

/// One bounded pass. The caller supplies one Holding pool and the composition
/// occurrence unit. Selection is limited to 20 exactly like the retained
/// generator, and the in-memory cursor advances between occurrences so one
/// schedule can generate its whole bounded backlog in a single pass.
pub async fn generate_due_occurrences<U: RoutineOccurrenceUnit + 'static>(
    pool: DatabasePool,
    now: i64,
    limit: i64,
    policy: OccurrencePolicy,
    unit: U,
) -> Result<OccurrencePass, Error> {
    let limit = limit.clamp(0, 20);
    let schedules = {
        let mut connection = pool.raw().acquire().await?;
        due_schedules(&mut connection, now, limit, &policy).await?
    };
    let mut unit = unit;
    let mut pass = OccurrencePass::default();
    for mut schedule in schedules {
        let mut cursor = schedule.next_occurrence_at;
        while cursor.is_some_and(|at| at <= now) && pass.examined < limit {
            let at = cursor.expect("checked Some");
            pass.examined += 1;
            let outcome = run_occurrence_unit(&pool, &mut unit, &schedule, at, now).await;
            match outcome {
                Ok(Some(result)) => {
                    if result.generated {
                        pass.generated += 1;
                    }
                    cursor = result.next;
                    schedule.next_occurrence_at = result.next;
                }
                Ok(None) => break,
                Err(error) => {
                    let mut connection = pool.raw().acquire().await?;
                    if policy.production
                        && !production_enabled(&mut connection, &schedule.company_id).await?
                    {
                        break;
                    }
                    let current = latest(&mut connection, &schedule.id).await?;
                    if current.as_ref().is_none_or(|current| {
                        current.next_occurrence_at != Some(at)
                            || current.version != schedule.version
                            || current.state != "active"
                    }) {
                        break;
                    }
                    let message = error_message(&error);
                    record_error(
                        &mut connection,
                        &schedule.id,
                        schedule.version,
                        &message,
                        now,
                    )
                    .await?;
                    eprintln!(
                        "{}",
                        serde_json::json!({
                            "event": "routine_generation_failed",
                            "scheduleId": schedule.id,
                            "companyId": schedule.company_id,
                            "occurrenceAt": at,
                            "lagMs": now - at,
                            "error": message,
                        })
                    );
                    break;
                }
            }
        }
    }
    let lag_ms = {
        let mut connection = pool.raw().acquire().await?;
        lag(&mut connection, now)
            .await?
            .map(|oldest| now - oldest)
            .unwrap_or(0)
    };
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "routine_generation",
            "generated": pass.generated,
            "examined": pass.examined,
            "lagMs": lag_ms,
        })
    );
    Ok(pass)
}

/// Read one stored authorization or refuse. Kept as a helper so composition
/// and tests share the exact retained error.
pub async fn require_authorization(
    connection: &mut PgConnection,
    schedule_id: &str,
    schedule_version: i64,
) -> Result<RoutineExecutionAuthorization, Error> {
    authorization(connection, schedule_id, schedule_version)
        .await?
        .ok_or_else(|| authorization_required().into())
}
