//! Transaction-bound Field intake authority.
use crate::public::field_visit_intake::{FieldIntakeAuthority, FieldIntakeScope};
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FILES_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FILES_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_ATTACHMENT_CHANGED",
        "The original Service visit or selected role changed before its evidence committed.",
    )
    .into()
}
/// An opaque witness scoped to one current actor, target and serializable unit.
pub struct CurrentFieldIntake<'a, A> {
    authority: &'a A,
    actor: &'a AuthorizedWorkContext,
    target: String,
    write: bool,
    unit: Unit,
    scope: FieldIntakeScope,
}
impl<A: FieldIntakeAuthority> CurrentFieldIntake<'_, A> {
    pub fn scope(&self) -> &FieldIntakeScope {
        &self.scope
    }
    pub async fn require_current(&self, c: &mut PgConnection, now: i64) -> Result<(), Error> {
        if unit(c).await? != self.unit || now >= self.actor.expires_at() {
            return Err(changed());
        }
        self.authority
            .revalidate(c, self.actor, &self.target, self.write)
            .await?;
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn prepare_field_intake<'a, A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    authority: &'a A,
    actor: &'a AuthorizedWorkContext,
    target: &str,
    write: bool,
    now: i64,
) -> Result<CurrentFieldIntake<'a, A>, Error> {
    let current_unit = unit(c).await?;
    if now >= actor.expires_at() {
        return Err(changed());
    }
    let scope = authority.authorize(c, actor, target, write).await?;
    if scope.company_id != actor.selection().workspace().company_id
        || unit(c).await? != current_unit
    {
        return Err(changed());
    }
    let guard = CurrentFieldIntake {
        authority,
        actor,
        target: target.to_owned(),
        write,
        unit: current_unit,
        scope,
    };
    guard.require_current(c, now).await?;
    Ok(guard)
}

// All potentially large source material stays inside PostgreSQL until these
// aggregate limits have passed. Limits apply to the complete image, including
// historical links and events, and deliberately reject rather than truncate.
async fn source_limit(c: &mut PgConnection, company: &str, id: &str) -> Result<(), Error> {
    let ok: Option<bool> = sqlx::query_scalar("SELECT octet_length(to_jsonb(s)::text)+4096 + COALESCE((SELECT sum(octet_length(to_jsonb(l)::text)+COALESCE(octet_length(to_jsonb(a)::text),0)+COALESCE(octet_length(to_jsonb(u)::text),0)+4096) FROM attachment_links l LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id WHERE l.set_id=s.id),0) + COALESCE((SELECT sum(octet_length(to_jsonb(e)::text)+4096) FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id),0) <= 2097152 AND (SELECT count(*) FROM attachment_links l WHERE l.set_id=s.id)+(SELECT count(*) FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id)<=1000 FROM attachment_sets s WHERE s.company_id=$1 AND s.id=$2").bind(company).bind(id).fetch_optional(c).await?;
    if ok != Some(true) {
        return Err(limit());
    }
    Ok(())
}
fn limit() -> Error {
    unavailable(
        "ATTACHMENT_READ_LIMIT",
        "The file source exceeds its bounded read.",
    )
}

async fn dto(c: &mut PgConnection, company: &str, id: &str) -> Result<serde_json::Value, Error> {
    source_limit(c, company, id).await?;
    let row: Option<serde_json::Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',s.id,'targetType',s.target_type,'targetId',s.target_id,'componentKind',s.component_kind,'componentRef',s.component_ref,'purposeKey',s.purpose_key,'expectedTargetVersion',s.expected_target_version,'policyVersion',s.policy_version,'classification',s.classification,'requiredCount',s.required_count,'maxFiles',s.max_files,'state',s.state,'createdAt',s.created_at,'updatedAt',s.updated_at,'links',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',l.id,'sourceId',l.attachment_id,'filename',COALESCE(a.original_name,u.original_name,'file'),'mediaType',COALESCE(a.media_type,u.media_type,'application/octet-stream'),'bytes',COALESCE(a.bytes,u.declared_bytes),'sourceState',a.source_state,'linkState',l.state,'caption',l.caption,'previewAvailable',a.preview_object_key IS NOT NULL,'createdAt',l.created_at,'expiresAt',l.expires_at,'boundAt',l.bound_at) ORDER BY l.created_at,l.id) FROM attachment_links l LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id WHERE l.set_id=s.id AND (l.attachment_id IS NULL OR a.id IS NOT NULL) AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)),'[]'::jsonb)) FROM attachment_sets s WHERE s.company_id=$1 AND s.id=$2").bind(company).bind(id).fetch_optional(c).await?;
    row.ok_or_else(changed)
}

async fn list_membership<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    guard: &CurrentFieldIntake<'_, A>,
    input: &crate::public::field_visit_intake::FieldSetListRequest,
    read_only: bool,
) -> Result<Vec<String>, Error> {
    let within: bool = sqlx::query_scalar("WITH selected AS MATERIALIZED (SELECT s.* FROM attachment_sets s WHERE s.company_id=$1 AND s.target_type='field_service_visit' AND s.target_id=$2 AND (s.state='bound' OR (NOT $3 AND s.state='draft' AND s.created_by=$4)) AND ($5::text IS NULL OR s.purpose_key=$5) AND ($6::text IS NULL OR s.component_kind=$6) AND ($7::bigint IS NULL OR s.expected_target_version=$7)), totals AS (SELECT count(*) sets, COALESCE(sum(octet_length(to_jsonb(s)::text)+4096 + COALESCE((SELECT sum(octet_length(to_jsonb(l)::text)+COALESCE(octet_length(to_jsonb(a)::text),0)+COALESCE(octet_length(to_jsonb(u)::text),0)+4096) FROM attachment_links l LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id WHERE l.set_id=s.id),0) + COALESCE((SELECT sum(octet_length(to_jsonb(e)::text)+4096) FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id),0)),0) bytes, COALESCE(sum((SELECT count(*) FROM attachment_links l WHERE l.set_id=s.id)+(SELECT count(*) FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id)),0) children FROM selected s) SELECT sets<=100 AND children<=1000 AND bytes<=2097152 FROM totals").bind(guard.scope.company_id.as_str()).bind(&guard.target).bind(read_only).bind(guard.actor.user_id().as_str()).bind(input.purpose_key.as_deref().filter(|s| !s.is_empty())).bind(input.component_kind.as_deref().filter(|s| !s.is_empty())).bind(input.expected_target_version.filter(|v| *v != 0)).fetch_one(&mut *c).await?;
    if !within {
        return Err(limit());
    }
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM attachment_sets WHERE company_id=$1 AND target_type='field_service_visit' AND target_id=$2 AND (state='bound' OR (NOT $3 AND state='draft' AND created_by=$4)) AND ($5::text IS NULL OR purpose_key=$5) AND ($6::text IS NULL OR component_kind=$6) AND ($7::bigint IS NULL OR expected_target_version=$7) ORDER BY created_at DESC,id DESC").bind(guard.scope.company_id.as_str()).bind(&guard.target).bind(read_only).bind(guard.actor.user_id().as_str()).bind(input.purpose_key.as_deref().filter(|s| !s.is_empty())).bind(input.component_kind.as_deref().filter(|s| !s.is_empty())).bind(input.expected_target_version.filter(|v| *v != 0)).fetch_all(&mut *c).await?;
    Ok(ids)
}

/// Metadata-only selection before source authorization. An empty Company-scoped
/// list reveals no Field source facts and preserves the existing empty-list
/// contract; nonempty results still require a current owner proof.
pub async fn field_set_query_has_rows(c:&mut PgConnection,actor:&AuthorizedWorkContext,input:&crate::public::field_visit_intake::FieldSetListRequest,read_only:bool)->Result<bool,Error>{
    unit(c).await?;
    actor.require_permission("records.read")?;
    if input.target_type!="field_service_visit" {return Err(changed());}
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_sets WHERE company_id=$1 AND target_type='field_service_visit' AND target_id=$2 AND (state='bound' OR (NOT $3 AND state='draft' AND created_by=$4)) AND ($5::text IS NULL OR purpose_key=$5) AND ($6::text IS NULL OR component_kind=$6) AND ($7::bigint IS NULL OR expected_target_version=$7))")
        .bind(actor.selection().workspace().company_id.as_str()).bind(&input.target_id).bind(read_only).bind(actor.user_id().as_str()).bind(input.purpose_key.as_deref().filter(|s|!s.is_empty())).bind(input.component_kind.as_deref().filter(|s|!s.is_empty())).bind(input.expected_target_version.filter(|v|*v!=0)).fetch_one(c).await?)
}

/// List the current actor's drafts and bound Field sets under a current read proof.
pub async fn list_field_sets<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    guard: &CurrentFieldIntake<'_, A>,
    input: &crate::public::field_visit_intake::FieldSetListRequest,
    read_only: bool,
    now: i64,
) -> Result<Vec<serde_json::Value>, Error> {
    guard.require_current(c, now).await?;
    if input.target_type != "field_service_visit" || input.target_id != guard.target {
        return Err(changed());
    }
    let ids = list_membership(c, guard, input, read_only).await?;
    let mut rows = Vec::with_capacity(ids.len());
    let mut images = Vec::with_capacity(ids.len());
    for id in &ids {
        let row = dto(c, guard.scope.company_id.as_str(), id).await?;
        if !crate::domain::field_visit_link::valid_field_purpose(
            "field_service_visit",
            row["componentKind"].as_str().unwrap_or(""),
            row["purposeKey"].as_str().unwrap_or(""),
        ) {
            return Err(invalid(
                "ATTACHMENT_POLICY_UNKNOWN",
                "This file purpose is not registered for the selected record and context.",
            ));
        }
        images.push(files_digest(c, guard.scope.company_id.as_str(), id).await?);
        rows.push(row);
    }
    guard.require_current(c, now).await?;
    if list_membership(c, guard, input, read_only).await? != ids {
        return Err(changed());
    }
    for (id, image) in ids.iter().zip(images) {
        if files_digest(c, guard.scope.company_id.as_str(), id).await? != image {
            return Err(changed());
        }
    }
    Ok(rows)
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::InvalidInput, code, message).into()
}
fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}
fn unavailable(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Unavailable, code, message).into()
}

async fn files_digest_excluding(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    excluded: Option<&str>,
) -> Result<String, Error> {
    source_limit(c, company, id).await?;
    sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_array(to_jsonb(s),(SELECT COALESCE(jsonb_agg(jsonb_build_array(to_jsonb(l),to_jsonb(a),to_jsonb(u)) ORDER BY l.id),'[]'::jsonb) FROM attachment_links l LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id WHERE l.set_id=s.id),(SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY e.id),'[]'::jsonb) FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id AND e.id IS DISTINCT FROM $3))::text,'UTF8')),'hex') FROM attachment_sets s WHERE s.id=$1 AND s.company_id=$2").bind(id).bind(company).bind(excluded).fetch_optional(c).await?.ok_or_else(changed)
}

async fn files_digest(c: &mut PgConnection, company: &str, id: &str) -> Result<String, Error> {
    files_digest_excluding(c, company, id, None).await
}

/// Create or replay a native set. Availability is supplied by trusted runtime configuration.
pub async fn create_field_set<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    read: &CurrentFieldIntake<'_, A>,
    write: Option<&CurrentFieldIntake<'_, A>>,
    input: &crate::public::field_visit_intake::FieldSetRequest,
    key: &str,
    availability: &crate::public::field_visit_intake::FieldIntakeAvailability,
    now: i64,
) -> Result<serde_json::Value, Error> {
    create_field_set_inner(
        c,
        read,
        write,
        input,
        key,
        availability,
        ApplyTiming {
            now,
            created_at: now,
            expected: None,
        },
    )
    .await
}

struct ApplyTiming<'a> {
    now: i64,
    created_at: i64,
    expected: Option<&'a FieldSetCreationPlan>,
}

async fn create_field_set_inner<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    read: &CurrentFieldIntake<'_, A>,
    write: Option<&CurrentFieldIntake<'_, A>>,
    input: &crate::public::field_visit_intake::FieldSetRequest,
    key: &str,
    availability: &crate::public::field_visit_intake::FieldIntakeAvailability,
    timing: ApplyTiming<'_>,
) -> Result<serde_json::Value, Error> {
    let ApplyTiming {
        now,
        created_at,
        expected,
    } = timing;
    use sqlx::Row;
    read.require_current(c, now).await?;
    if availability.uploads_frozen {
        return Err(unavailable(
            "ATTACHMENT_UPLOAD_FREEZE",
            "File intake is briefly paused for storage reconciliation.",
        ));
    }
    let classification = crate::application::field_visit_intake::policy(input)?;
    if input.target_id != read.target {
        return Err(changed());
    }
    let component =
        crate::domain::field_visit_intake::component_ref(input.component_ref.as_deref());
    let id = crate::domain::field_visit_intake::field_set_id(
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        key,
    );
    if let Some(plan) = expected {
        plan.require_preimage(c).await?;
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "attachment-set:{}:{}:{}",
            read.scope.company_id.as_str(),
            read.actor.user_id().as_str(),
            key
        ))
        .execute(&mut *c)
        .await?;
    let prior_id: Option<Option<String>> = sqlx::query_scalar("SELECT CASE WHEN octet_length(s.id)<=512 THEN s.id END FROM attachment_sets s WHERE s.company_id=$1 AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e JOIN attachment_sets source ON source.id=e.set_id AND source.company_id=e.company_id AND source.created_by=e.actor_id WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' AND e.details_json::jsonb->>'requestedSetId'=$2)) LIMIT 1").bind(read.scope.company_id.as_str()).bind(&id).bind(format!("set-alias-{id}")).bind(read.actor.user_id().as_str()).fetch_optional(&mut *c).await?;
    let prior = if let Some(prior_id) = prior_id {
        let prior_id = prior_id.ok_or_else(limit)?;
        source_limit(c, read.scope.company_id.as_str(), &prior_id).await?;
        Some(sqlx::query("SELECT s.*, EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id AND e.action='set.created' AND COALESCE((e.details_json::jsonb->>'compatibilityAdapter')::boolean,false)) compatibility FROM attachment_sets s WHERE s.company_id=$1 AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e JOIN attachment_sets source ON source.id=e.set_id AND source.company_id=e.company_id AND source.created_by=e.actor_id WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' AND e.details_json::jsonb->>'requestedSetId'=$2)) LIMIT 1").bind(read.scope.company_id.as_str()).bind(&id).bind(format!("set-alias-{id}")).bind(read.actor.user_id().as_str()).fetch_optional(&mut *c).await?.ok_or_else(changed)?)
    } else {
        None
    };
    if let Some(row) = prior {
        let same = row.try_get::<String, _>("target_type")? == input.target_type
            && row.try_get::<String, _>("target_id")? == input.target_id
            && row.try_get::<String, _>("component_kind")? == input.component_kind
            && row
                .try_get::<Option<String>, _>("component_ref")?
                .as_deref()
                == component
            && row.try_get::<String, _>("purpose_key")? == input.purpose_key
            && row.try_get::<i64, _>("expected_target_version")? == input.expected_target_version
            && !row.try_get::<bool, _>("compatibility")?;
        if !same {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key was already used for another attachment set.",
            ));
        }
        let actual = row.try_get::<String, _>("id")?;
        let before = files_digest(c, read.scope.company_id.as_str(), &actual).await?;
        let result = dto(c, read.scope.company_id.as_str(), &actual).await?;
        read.require_current(c, now).await?;
        if files_digest(c, read.scope.company_id.as_str(), &actual).await? != before {
            return Err(changed());
        }
        return Ok(result);
    }
    let write = write.ok_or_else(changed)?;
    if !write.write
        || write.target != read.target
        || write.unit != read.unit
        || !std::ptr::eq(write.actor, read.actor)
        || write.scope != read.scope
    {
        return Err(changed());
    }
    write.require_current(c, now).await?;
    if let Some(plan) = expected {
        plan.require_preimage(c).await?;
    }
    if write.scope.version != input.expected_target_version {
        return Err(conflict(
            "ATTACHMENT_TARGET_CHANGED",
            "The record changed. Refresh before adding files.",
        ));
    }
    crate::domain::field_visit_intake::validate_component_ref(component)?;
    if !availability.scanner_ready || !availability.preview_ready {
        return Err(unavailable(
            "ATTACHMENT_SCANNER_UNAVAILABLE",
            "File intake is unavailable until the malware scanner and current signatures are ready.",
        ));
    }
    let reusable: Option<Option<String>> = sqlx::query_scalar("SELECT CASE WHEN octet_length(candidate.id)<=512 THEN candidate.id END FROM attachment_sets candidate WHERE candidate.company_id=$1 AND candidate.target_type=$2 AND candidate.target_id=$3 AND candidate.component_kind=$4 AND COALESCE(candidate.component_ref,'')=COALESCE($5,'') AND candidate.purpose_key=$6 AND candidate.expected_target_version=$7 AND candidate.created_by=$8 AND candidate.state='draft' AND EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=candidate.id AND e.company_id=candidate.company_id AND e.action='set.created' AND COALESCE((e.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=false)").bind(read.scope.company_id.as_str()).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).bind(read.actor.user_id().as_str()).fetch_optional(&mut *c).await?;
    let reusable = reusable.map(|id| id.ok_or_else(limit)).transpose()?;
    let reused = reusable.is_some();
    let event_id = expected
        .map(|plan| plan.event_id.clone())
        .unwrap_or_else(|| format!("attachment-event-{}", uuid::Uuid::new_v4()));
    let actual = if let Some(existing) = reusable {
        source_limit(c, read.scope.company_id.as_str(), &existing).await?;
        let alias_id = format!("set-alias-{id}");
        let before = files_digest_excluding(
            c,
            read.scope.company_id.as_str(),
            &existing,
            Some(&alias_id),
        )
        .await?;
        let details = serde_json::json!({"requestedSetId":id}).to_string();
        let result=sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) SELECT $1,company_id,id,$2,'set.reused',$3,$4 FROM attachment_sets WHERE id=$5 AND company_id=$6 AND created_by=$2 AND state='draft'").bind(format!("set-alias-{id}")).bind(read.actor.user_id().as_str()).bind(details).bind(created_at).bind(&existing).bind(read.scope.company_id.as_str()).execute(&mut *c).await?;
        if result.rows_affected() != 1
            || files_digest_excluding(
                c,
                read.scope.company_id.as_str(),
                &existing,
                Some(&alias_id),
            )
            .await?
                != before
        {
            return Err(changed());
        }
        existing
    } else {
        let audience =
            serde_json::json!({"classification":classification,"policyVersion":1}).to_string();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,retention_policy_key,retention_policy_version,retain_until,destruction_mode,required_count,max_files,state,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,$11,$12,NULL,NULL,NULL,NULL,0,10,'draft',$13,$14,$14)").bind(&id).bind(read.scope.company_id.as_str()).bind(&write.scope.branch_id).bind(&write.scope.department_id).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).bind(classification).bind(audience).bind(read.actor.user_id().as_str()).bind(created_at).execute(&mut *c).await?;
        let details=serde_json::json!({"targetType":input.target_type,"targetId":input.target_id,"purposeKey":input.purpose_key,"expectedTargetVersion":input.expected_target_version,"compatibilityAdapter":false}).to_string();
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.created',$5,$6)").bind(&event_id).bind(read.scope.company_id.as_str()).bind(&id).bind(read.actor.user_id().as_str()).bind(details).bind(created_at).execute(&mut *c).await?;
        id.clone()
    };
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_sets s WHERE s.id=$1 AND s.company_id=$2 AND s.target_type=$3 AND s.target_id=$4 AND s.component_kind=$5 AND s.component_ref IS NOT DISTINCT FROM $6 AND s.purpose_key=$7 AND s.expected_target_version=$8 AND s.created_by=$9 AND s.state='draft' AND (($10 AND EXISTS(SELECT 1 FROM attachment_events e WHERE e.id=$11 AND e.company_id=s.company_id AND e.set_id=s.id AND e.actor_id=$9 AND e.action='set.reused' AND e.attachment_id IS NULL AND e.link_id IS NULL AND e.details_json::jsonb=jsonb_build_object('requestedSetId',$12::text) AND e.created_at=$13)) OR (NOT $10 AND s.branch_id IS NOT DISTINCT FROM $14 AND s.department_id IS NOT DISTINCT FROM $15 AND s.policy_version=1 AND s.classification=$16 AND s.audience_json::jsonb=jsonb_build_object('classification',$16::text,'policyVersion',1) AND s.retention_policy_key IS NULL AND s.retention_policy_version IS NULL AND s.retain_until IS NULL AND s.destruction_mode IS NULL AND s.required_count=0 AND s.max_files=10 AND s.created_at=$13 AND s.updated_at=$13 AND s.bound_at IS NULL AND s.bound_command_id IS NULL AND NOT EXISTS(SELECT 1 FROM attachment_links l WHERE l.set_id=s.id) AND (SELECT count(*) FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id)=1 AND EXISTS(SELECT 1 FROM attachment_events e WHERE e.id=$17 AND e.company_id=s.company_id AND e.set_id=s.id AND e.actor_id=$9 AND e.attachment_id IS NULL AND e.link_id IS NULL AND e.action='set.created' AND e.details_json::jsonb=jsonb_build_object('targetType',$3::text,'targetId',$4::text,'purposeKey',$7::text,'expectedTargetVersion',$8::bigint,'compatibilityAdapter',false) AND e.created_at=$13))))").bind(&actual).bind(read.scope.company_id.as_str()).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).bind(read.actor.user_id().as_str()).bind(reused).bind(format!("set-alias-{id}")).bind(&id).bind(created_at).bind(&write.scope.branch_id).bind(&write.scope.department_id).bind(classification).bind(&event_id).fetch_one(&mut *c).await?;
    if !valid {
        return Err(changed());
    }
    let before = files_digest(c, read.scope.company_id.as_str(), &actual).await?;
    let result = dto(c, read.scope.company_id.as_str(), &actual).await?;
    write.require_current(c, now).await?;
    read.require_current(c, now).await?;
    if files_digest(c, read.scope.company_id.as_str(), &actual).await? != before {
        return Err(changed());
    }
    Ok(result)
}

/// Read-only disposition selected under the current Field and Files proofs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldSetDisposition {
    Replay,
    Reuse,
    New,
}

/// Opaque, transaction-bound prediction. It performs no canonical writes.
pub struct FieldSetCreationPlan {
    unit: Unit,
    company: String,
    actor: String,
    actor_user: String,
    scope: FieldIntakeScope,
    probe: String,
    event_id: String,
    input: crate::public::field_visit_intake::FieldSetRequest,
    key: String,
    availability: crate::public::field_visit_intake::FieldIntakeAvailability,
    now: i64,
    disposition: FieldSetDisposition,
    set_id: String,
    policy_version: i64,
    image: Option<String>,
}
impl FieldSetCreationPlan {
    pub fn set_id(&self) -> &str {
        &self.set_id
    }
    pub fn expected_target_version(&self) -> i64 {
        self.input.expected_target_version
    }
    pub fn policy_version(&self) -> i64 {
        self.policy_version
    }
    pub fn disposition(&self) -> FieldSetDisposition {
        self.disposition
    }
    async fn require_preimage(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit {
            return Err(changed());
        }
        let component =
            crate::domain::field_visit_intake::component_ref(self.input.component_ref.as_deref());
        let requested = crate::domain::field_visit_intake::field_set_id(
            &self.company,
            &self.actor_user,
            &self.key,
        );
        if planning_probe(
            c,
            &self.company,
            &self.actor_user,
            &requested,
            &self.input,
            component,
        )
        .await?
            != self.probe
        {
            return Err(changed());
        }
        if let Some(expected) = &self.image
            && files_digest(c, &self.company, &self.set_id).await? != *expected
        {
            return Err(changed());
        }
        Ok(())
    }

    /// Apply a native Workflow command and append its immutable Files origin.
    pub async fn apply_native<A: FieldIntakeAuthority>(
        &self,
        c: &mut PgConnection,
        read: &CurrentFieldIntake<'_, A>,
        write: Option<&CurrentFieldIntake<'_, A>>,
        now: i64,
        evidence: &erpx_server_workflow::public::field_intake_command_evidence::PlannedFieldIntakeCommandEvidence,
    ) -> Result<WrittenFieldSet, Error> {
        use erpx_server_workflow::public::field_intake_command_evidence::FieldIntakeOriginFacts;
        let disposition = match self.disposition {
            FieldSetDisposition::New => "new",
            FieldSetDisposition::Reuse => "reuse",
            FieldSetDisposition::Replay => return Err(changed()),
        };
        let ledger_key = crate::domain::field_visit_intake::field_set_command_id(
            &self.company,
            &self.actor_user,
            &self.key,
        );
        let facts = || FieldIntakeOriginFacts {
            key: &ledger_key,
            company: &self.company,
            actor: &self.actor_user,
            target: &self.input.target_id,
            set_id: &self.set_id,
            version: self.input.expected_target_version,
            policy_version: self.policy_version,
        };
        let anchor = evidence.verify_field_intake_origin(c, facts()).await?;
        let mut written = self.apply(c, read, write, now).await?;
        let requested = crate::domain::field_visit_intake::field_set_id(
            &self.company,
            &self.actor_user,
            &self.key,
        );
        let event_id = format!("set-native-{requested}");
        let details = serde_json::json!({"commandId":ledger_key,"wireKey":self.key,"requestedSetId":requested,"disposition":disposition,"receiptHash":anchor.receipt_hash,"auditId":anchor.audit_id,"outboxId":anchor.outbox_id});
        sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES($1,$2,$3,$4,'set.native-origin',$5,$6)").bind(&event_id).bind(&self.company).bind(&self.set_id).bind(&self.actor_user).bind(details.to_string()).bind(self.now).execute(&mut *c).await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_events e WHERE e.id=$1 AND e.company_id=$2 AND e.set_id=$3 AND e.actor_id=$4 AND e.action='set.native-origin' AND e.attachment_id IS NULL AND e.link_id IS NULL AND e.details_json::jsonb=$5 AND e.created_at=$6)").bind(&event_id).bind(&self.company).bind(&self.set_id).bind(&self.actor_user).bind(details).bind(self.now).fetch_one(&mut *c).await?;
        if !valid
            || files_digest_excluding(c, &self.company, &self.set_id, Some(&event_id)).await?
                != written.image
        {
            return Err(changed());
        }
        if evidence.verify_field_intake_origin(c, facts()).await? != anchor {
            return Err(changed());
        }
        read.require_current(c, now).await?;
        write.ok_or_else(changed)?.require_current(c, now).await?;
        if files_digest_excluding(c, &self.company, &self.set_id, Some(&event_id)).await?
            != written.image
        {
            return Err(changed());
        }
        written.image = files_digest(c, &self.company, &self.set_id).await?;
        Ok(written)
    }

    /// Re-evaluate the bounded prediction, then apply exactly once in this unit.
    pub async fn apply<A: FieldIntakeAuthority>(
        &self,
        c: &mut PgConnection,
        read: &CurrentFieldIntake<'_, A>,
        write: Option<&CurrentFieldIntake<'_, A>>,
        now: i64,
    ) -> Result<WrittenFieldSet, Error> {
        let current = prepare_field_set_creation(
            c,
            read,
            write,
            &self.input,
            &self.key,
            &self.availability,
            now,
        )
        .await?;
        if self.unit != current.unit
            || self.company != current.company
            || self.actor != current.actor
            || self.scope != current.scope
            || self.probe != current.probe
            || self.disposition != current.disposition
            || self.set_id != current.set_id
            || self.policy_version != current.policy_version
            || self.image != current.image
        {
            return Err(changed());
        }
        let data = create_field_set_inner(
            c,
            read,
            write,
            &self.input,
            &self.key,
            &self.availability,
            ApplyTiming {
                now,
                created_at: self.now,
                expected: Some(self),
            },
        )
        .await?;
        if data["id"].as_str() != Some(&self.set_id)
            || data["policyVersion"].as_i64() != Some(self.policy_version)
            || data["expectedTargetVersion"].as_i64() != Some(self.input.expected_target_version)
        {
            return Err(changed());
        }
        let image = files_digest(c, &self.company, &self.set_id).await?;
        Ok(WrittenFieldSet {
            unit: self.unit.clone(),
            company: self.company.clone(),
            set_id: self.set_id.clone(),
            image,
            data,
        })
    }
}

/// Files-issued complete image retained across workflow audit/outbox flushing.
pub struct WrittenFieldSet {
    unit: Unit,
    company: String,
    set_id: String,
    image: String,
    data: serde_json::Value,
}
impl WrittenFieldSet {
    pub fn data(&self) -> &serde_json::Value {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || files_digest(c, &self.company, &self.set_id).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}

async fn planning_probe(
    c: &mut PgConnection,
    company: &str,
    actor: &str,
    requested: &str,
    input: &crate::public::field_visit_intake::FieldSetRequest,
    component: Option<&str>,
) -> Result<String, Error> {
    let within: bool = sqlx::query_scalar("WITH selected AS MATERIALIZED (SELECT s.* FROM attachment_sets s WHERE s.company_id=$1 AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e JOIN attachment_sets source ON source.id=e.set_id AND source.company_id=e.company_id AND source.created_by=e.actor_id WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' AND e.details_json::jsonb->>'requestedSetId'=$2) OR (s.target_type=$5 AND s.target_id=$6 AND s.component_kind=$7 AND COALESCE(s.component_ref,'')=COALESCE($8,'') AND s.purpose_key=$9 AND s.expected_target_version=$10 AND s.created_by=$4 AND s.state='draft'))) SELECT count(*)<=100 AND COALESCE(sum(octet_length(to_jsonb(s)::text)+4096),0)+COALESCE((SELECT octet_length(to_jsonb(e)::text)+4096 FROM attachment_events e WHERE e.id=$3),0)<=2097152 FROM selected s").bind(company).bind(requested).bind(format!("set-alias-{requested}")).bind(actor).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).fetch_one(&mut *c).await?;
    if !within {
        return Err(limit());
    }
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_array((SELECT COALESCE(jsonb_agg(to_jsonb(s) ORDER BY s.id),'[]'::jsonb) FROM attachment_sets s WHERE s.company_id=$1 AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e JOIN attachment_sets source ON source.id=e.set_id AND source.company_id=e.company_id AND source.created_by=e.actor_id WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' AND e.details_json::jsonb->>'requestedSetId'=$2) OR (s.target_type=$5 AND s.target_id=$6 AND s.component_kind=$7 AND COALESCE(s.component_ref,'')=COALESCE($8,'') AND s.purpose_key=$9 AND s.expected_target_version=$10 AND s.created_by=$4 AND s.state='draft'))),(SELECT to_jsonb(e) FROM attachment_events e WHERE e.id=$3))::text,'UTF8')),'hex')").bind(company).bind(requested).bind(format!("set-alias-{requested}")).bind(actor).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).fetch_one(c).await?)
}
fn context_identity(actor: &AuthorizedWorkContext) -> String {
    format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        actor.holding_id().as_str(),
        actor.user_id().as_str(),
        actor.session_id().as_str(),
        actor.principal_id(),
        actor.identity_version(),
        actor.selection().revision(),
        actor.selection().role_revision(),
        actor.selection().assignment().id.as_str(),
        actor.selection().assignment().version,
        actor.selection().assignment().role,
        actor
            .selection()
            .workspace()
            .branch_id
            .as_ref()
            .map_or("", |id| id.as_str()),
        actor
            .selection()
            .workspace()
            .department_id
            .as_ref()
            .map_or("", |id| id.as_str())
    )
}

/// Predict the existing, reusable, or new set without creating sets or receipts.
pub async fn prepare_field_set_creation<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    read: &CurrentFieldIntake<'_, A>,
    write: Option<&CurrentFieldIntake<'_, A>>,
    input: &crate::public::field_visit_intake::FieldSetRequest,
    key: &str,
    availability: &crate::public::field_visit_intake::FieldIntakeAvailability,
    now: i64,
) -> Result<FieldSetCreationPlan, Error> {
    use sqlx::Row;
    read.require_current(c, now).await?;
    if availability.uploads_frozen {
        return Err(unavailable(
            "ATTACHMENT_UPLOAD_FREEZE",
            "File intake is briefly paused for storage reconciliation.",
        ));
    }
    crate::application::field_visit_intake::policy(input)?;
    if input.target_id != read.target {
        return Err(changed());
    }
    let component =
        crate::domain::field_visit_intake::component_ref(input.component_ref.as_deref());
    let requested = crate::domain::field_visit_intake::field_set_id(
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        key,
    );
    let probe = planning_probe(
        c,
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        &requested,
        input,
        component,
    )
    .await?;
    let prior: Option<Option<String>> = sqlx::query_scalar("SELECT CASE WHEN octet_length(s.id)<=512 THEN s.id END FROM attachment_sets s WHERE s.company_id=$1 AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e JOIN attachment_sets source ON source.id=e.set_id AND source.company_id=e.company_id AND source.created_by=e.actor_id WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' AND e.details_json::jsonb->>'requestedSetId'=$2)) LIMIT 1").bind(read.scope.company_id.as_str()).bind(&requested).bind(format!("set-alias-{requested}")).bind(read.actor.user_id().as_str()).fetch_optional(&mut *c).await?;
    let (disposition, set_id, policy_version, image) = if let Some(id) = prior {
        let id = id.ok_or_else(limit)?;
        source_limit(c, read.scope.company_id.as_str(), &id).await?;
        let row = sqlx::query("SELECT s.policy_version,(s.target_type=$3 AND s.target_id=$4 AND s.component_kind=$5 AND s.component_ref IS NOT DISTINCT FROM $6 AND s.purpose_key=$7 AND s.expected_target_version=$8 AND NOT EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=s.id AND e.company_id=s.company_id AND e.action='set.created' AND COALESCE((e.details_json::jsonb->>'compatibilityAdapter')::boolean,false))) same FROM attachment_sets s WHERE s.company_id=$1 AND s.id=$2").bind(read.scope.company_id.as_str()).bind(&id).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).fetch_one(&mut *c).await?;
        if !row.try_get::<bool, _>("same")? {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key was already used for another attachment set.",
            ));
        }
        let image = files_digest(c, read.scope.company_id.as_str(), &id).await?;
        (
            FieldSetDisposition::Replay,
            id,
            row.try_get("policy_version")?,
            Some(image),
        )
    } else {
        let write = write.ok_or_else(changed)?;
        if !write.write
            || write.target != read.target
            || write.unit != read.unit
            || !std::ptr::eq(write.actor, read.actor)
            || write.scope != read.scope
        {
            return Err(changed());
        }
        write.require_current(c, now).await?;
        if write.scope.version != input.expected_target_version {
            return Err(conflict(
                "ATTACHMENT_TARGET_CHANGED",
                "The record changed. Refresh before adding files.",
            ));
        }
        crate::domain::field_visit_intake::validate_component_ref(component)?;
        if !availability.scanner_ready || !availability.preview_ready {
            return Err(unavailable(
                "ATTACHMENT_SCANNER_UNAVAILABLE",
                "File intake is unavailable until the malware scanner and current signatures are ready.",
            ));
        }
        let reusable: Option<Option<String>> = sqlx::query_scalar("SELECT CASE WHEN octet_length(candidate.id)<=512 THEN candidate.id END FROM attachment_sets candidate WHERE candidate.company_id=$1 AND candidate.target_type=$2 AND candidate.target_id=$3 AND candidate.component_kind=$4 AND COALESCE(candidate.component_ref,'')=COALESCE($5,'') AND candidate.purpose_key=$6 AND candidate.expected_target_version=$7 AND candidate.created_by=$8 AND candidate.state='draft' AND EXISTS(SELECT 1 FROM attachment_events e WHERE e.set_id=candidate.id AND e.company_id=candidate.company_id AND e.action='set.created' AND COALESCE((e.details_json::jsonb->>'compatibilityAdapter')::boolean,false)=false)").bind(read.scope.company_id.as_str()).bind(&input.target_type).bind(&input.target_id).bind(&input.component_kind).bind(component).bind(&input.purpose_key).bind(input.expected_target_version).bind(read.actor.user_id().as_str()).fetch_optional(&mut *c).await?;
        if let Some(id) = reusable {
            let id = id.ok_or_else(limit)?;
            source_limit(c, read.scope.company_id.as_str(), &id).await?;
            let version: i64 = sqlx::query_scalar(
                "SELECT policy_version FROM attachment_sets WHERE company_id=$1 AND id=$2",
            )
            .bind(read.scope.company_id.as_str())
            .bind(&id)
            .fetch_one(&mut *c)
            .await?;
            let image = files_digest(c, read.scope.company_id.as_str(), &id).await?;
            (FieldSetDisposition::Reuse, id, version, Some(image))
        } else {
            (FieldSetDisposition::New, requested.clone(), 1, None)
        }
    };
    if disposition != FieldSetDisposition::Replay {
        write.ok_or_else(changed)?.require_current(c, now).await?;
    }
    read.require_current(c, now).await?;
    if planning_probe(
        c,
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        &requested,
        input,
        component,
    )
    .await?
        != probe
    {
        return Err(changed());
    }
    if let Some(expected) = &image
        && files_digest(c, read.scope.company_id.as_str(), &set_id).await? != *expected
    {
        return Err(changed());
    }
    Ok(FieldSetCreationPlan {
        unit: unit(c).await?,
        company: read.scope.company_id.as_str().into(),
        actor: context_identity(read.actor),
        actor_user: read.actor.user_id().as_str().into(),
        scope: read.scope.clone(),
        probe,
        event_id: format!("attachment-event-{}", uuid::Uuid::new_v4()),
        input: input.clone(),
        key: key.into(),
        availability: availability.clone(),
        now,
        disposition,
        set_id,
        policy_version,
        image,
    })
}

/// Classification of an exact recorded Files key. Native origins require the matching Workflow evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordedFieldSetKind {
    Legacy,
    Native,
}
pub struct RecordedFieldSet {
    pub kind: RecordedFieldSetKind,
    pub data: serde_json::Value,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub anchor: Option<
        erpx_server_workflow::public::field_intake_command_evidence::FieldIntakeReceiptAnchor,
    >,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeOriginDetails {
    command_id: String,
    wire_key: String,
    requested_set_id: String,
    disposition: String,
    receipt_hash: String,
    audit_id: String,
    outbox_id: String,
}

async fn recorded_identity(
    c: &mut PgConnection,
    company: &str,
    actor: &str,
    requested: &str,
) -> Result<(Option<Option<String>>, bool), Error> {
    let prior: Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(s.id)<=512 THEN s.id END FROM attachment_sets s WHERE s.company_id=$1 AND (s.id=$2 OR s.id=(SELECT e.set_id FROM attachment_events e JOIN attachment_sets source ON source.id=e.set_id AND source.company_id=e.company_id AND source.created_by=e.actor_id WHERE e.id=$3 AND e.company_id=$1 AND e.actor_id=$4 AND e.action='set.reused' AND e.details_json::jsonb->>'requestedSetId'=$2)) LIMIT 1").bind(company).bind(requested).bind(format!("set-alias-{requested}")).bind(actor).fetch_optional(&mut *c).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachment_events WHERE id=$1)")
            .bind(format!("set-native-{requested}"))
            .fetch_one(c)
            .await?;
    Ok((prior, exists))
}
/// Read-only lookup: absence is distinct from authorization or receipt errors.
pub async fn recorded_field_set<A: FieldIntakeAuthority>(
    c: &mut PgConnection,
    read: &CurrentFieldIntake<'_, A>,
    input: &crate::public::field_visit_intake::FieldSetRequest,
    key: &str,
    availability: &crate::public::field_visit_intake::FieldIntakeAvailability,
    now: i64,
) -> Result<Option<RecordedFieldSet>, Error> {
    read.require_current(c, now).await?;
    if availability.uploads_frozen {
        return Err(unavailable(
            "ATTACHMENT_UPLOAD_FREEZE",
            "File intake is briefly paused for storage reconciliation.",
        ));
    }
    crate::application::field_visit_intake::policy(input)?;
    if input.target_id != read.target {
        return Err(changed());
    }
    let requested = crate::domain::field_visit_intake::field_set_id(
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        key,
    );
    let event_id = format!("set-native-{requested}");
    let identity = recorded_identity(
        c,
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        &requested,
    )
    .await?;
    let (prior, exists) = identity.clone();
    let Some(id) = prior else {
        if exists {
            return Err(changed());
        }
        read.require_current(c, now).await?;
        if recorded_identity(
            c,
            read.scope.company_id.as_str(),
            read.actor.user_id().as_str(),
            &requested,
        )
        .await?
            != identity
        {
            return Err(changed());
        }
        return Ok(None);
    };
    let id = id.ok_or_else(limit)?;
    source_limit(c, read.scope.company_id.as_str(), &id).await?;
    let (kind, anchor) = if exists {
        let raw: Option<String>=sqlx::query_scalar("SELECT CASE WHEN octet_length(e.details_json)<=4096 THEN e.details_json END FROM attachment_events e WHERE e.id=$1 AND e.company_id=$2 AND e.set_id=$3 AND e.actor_id=$4 AND e.action='set.native-origin' AND e.attachment_id IS NULL AND e.link_id IS NULL AND e.created_at BETWEEN 0 AND 9007199254740991").bind(&event_id).bind(read.scope.company_id.as_str()).bind(&id).bind(read.actor.user_id().as_str()).fetch_optional(&mut *c).await?.flatten();
        let details: NativeOriginDetails =
            serde_json::from_str(&raw.ok_or_else(changed)?).map_err(|_| changed())?;
        if details.command_id
            != crate::domain::field_visit_intake::field_set_command_id(
                read.scope.company_id.as_str(),
                read.actor.user_id().as_str(),
                key,
            )
            || details.wire_key != key
            || details.requested_set_id != requested
            || !matches!(details.disposition.as_str(), "new" | "reuse")
            || details.receipt_hash.len() != 64
            || !details
                .receipt_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || details.audit_id.is_empty()
            || details.audit_id.len() > 512
            || details.outbox_id.is_empty()
            || details.outbox_id.len() > 512
        {
            return Err(changed());
        }
        (RecordedFieldSetKind::Native,Some(erpx_server_workflow::public::field_intake_command_evidence::FieldIntakeReceiptAnchor {receipt_hash:details.receipt_hash,audit_id:details.audit_id,outbox_id:details.outbox_id}))
    } else {
        (RecordedFieldSetKind::Legacy, None)
    };
    let data = create_field_set(c, read, None, input, key, availability, now).await?;
    if data["id"].as_str() != Some(&id) {
        return Err(changed());
    }
    let (branch_id, department_id): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT branch_id,department_id FROM attachment_sets WHERE company_id=$1 AND id=$2",
    )
    .bind(read.scope.company_id.as_str())
    .bind(&id)
    .fetch_one(&mut *c)
    .await?;
    let image = files_digest(c, read.scope.company_id.as_str(), &id).await?;
    read.require_current(c, now).await?;
    if recorded_identity(
        c,
        read.scope.company_id.as_str(),
        read.actor.user_id().as_str(),
        &requested,
    )
    .await?
        != identity
        || files_digest(c, read.scope.company_id.as_str(), &id).await? != image
    {
        return Err(changed());
    }
    Ok(Some(RecordedFieldSet {
        kind,
        data,
        branch_id,
        department_id,
        anchor,
    }))
}
