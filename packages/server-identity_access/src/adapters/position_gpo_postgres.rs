//! PostgreSQL adapter for the native Position GPO mapping/delivery slice.
//!
//! SQL is Identity-owned (`position_gpo_policies`, `position_gpo_revisions`,
//! `position_gpo_jobs`, `position_gpo_config_ownership`, `iam_connections`,
//! `iam_members`, `iam_identities`, `iam_mfa_sessions`, `iam_mfa`, `assignments`,
//! `active_work_contexts`) plus the shared command receipt, audit and outbox
//! tables and read-only projections of the Organization/People owner rows the
//! migration screen shows (`organization_positions`, `organization_gpo_rules`,
//! `hr_people`, `hr_engagements`, `people_directory_state`,
//! `people_directory_membership_sources`, `organization_gpo_membership_claims`).
//! No Organization or People row is mutated. The caller owns the serializable
//! Holding transaction and commits it; this adapter never commits.
use crate::application::position_gpo::{
    EffectiveRule, GpoFuture, GpoReceiptSnapshot, PositionGpoRepository, PublishPlan, UnmapPlan,
};
use crate::domain::iam_roles::IamRoleActor;
use crate::domain::position_gpo::{
    idempotency_conflict, readback_invalid, version_conflict, ClaimRow, ConfigRetryInput,
    ConfigRow, ConnectionRow, DesiredRow, DirectoryState, GpoAuthority, HolderRow, JobRetryInput,
    JobRow, JobStateRow, MappingRow, OwnershipRow, PolicyRow, PositionFacts, RuleStatusRow,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{json, Value};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const RECORD_TYPE: &str = "technology_position-gpo";

fn source_invalid() -> BusinessError {
    crate::domain::position_gpo::source_invalid()
}

fn random_identifier() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        static FALLBACK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let value = FALLBACK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return format!("fallback-{value}");
    }
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub struct PgPositionGpo<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgPositionGpo<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("POSITION_GPO_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("POSITION_GPO_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn effects(
        &mut self,
        actor: &IamRoleActor,
        key: &str,
        request_hash: &str,
        operation: &str,
        record_id: &str,
        record_version: i64,
        result: &Value,
        now: i64,
    ) -> Result<(), Error> {
        let authority = actor.authority_json();
        let receipt = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(record_id)
        .bind(result.to_string())
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if receipt != 1 {
            return Err(idempotency_conflict().into());
        }
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(operation)
        .bind(RECORD_TYPE)
        .bind(record_id)
        .bind(record_version)
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        let payload = json!({
            "authority": serde_json::from_str::<Value>(&authority).map_err(|_| readback_invalid())?,
            "id": record_id,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "version": record_version,
        });
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(operation)
        .bind(RECORD_TYPE)
        .bind(record_id)
        .bind(record_version)
        .bind(payload.to_string())
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

const JOB_COLUMNS: &str = "id,company_id,position_id,person_id,engagement_id,account_id,\
 revision::bigint revision,engagement_version::bigint engagement_version,connection_id,gpo_id,\
 filter_group_id,computer_target_ou_id,desired_action,status,version::bigint version,rule_id,\
 source_id";

impl PositionGpoRepository for PgPositionGpo<'_> {
    type Error = Error;

    fn failure_code(error: &Self::Error) -> &'static str {
        match error {
            PgTransactionError::Business(error) => error.code,
            _ => "DIRECTORY_RETRY_REQUIRED",
        }
    }

    fn identifier(&mut self) -> String {
        random_identifier()
    }

    fn authority<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        now: i64,
    ) -> GpoFuture<'a, GpoAuthority, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT \
                 EXISTS(SELECT 1 FROM assignments a \
                  JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id \
                  JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' \
                  JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
                   AND v.revision=r.published_revision \
                  WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.role=$6 \
                  AND a.scope_kind=$8 AND a.revoked_at IS NULL \
                  AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
                  AND iam_role_allows(a.role,a.company_id,$7) \
                  AND ($9::text IS NULL OR a.role=$9::text) \
                  AND m.status='active' \
                  AND EXISTS(SELECT 1 FROM active_work_contexts w WHERE w.session_id=$5 \
                   AND w.company_id=$3 AND w.assignment_id=a.id)) it_holding",
            )
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(actor.session_id.as_str())
            .bind(crate::domain::position_gpo::IT_ROLE)
            .bind(crate::domain::position_gpo::DIRECTORY_APPLY_PERMISSION)
            .bind(crate::domain::position_gpo::HOLDING_SCOPE)
            .bind(actor.role.as_deref())
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(GpoAuthority {
                it_holding: row.try_get("it_holding")?,
            })
        })
    }

    fn lock_catalog<'a>(&'a mut self) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            sqlx::query("SELECT pg_advisory_xact_lock(821039320)")
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> GpoFuture<'a, Option<GpoReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(GpoReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    authority_json: row
                        .try_get::<Option<String>, _>("authority_json")?
                        .unwrap_or_default(),
                })
            })
            .transpose()
        })
    }

    fn active_position<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<PositionFacts>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,title FROM organization_positions \
                 WHERE id=$1 AND company_id=$2 AND active=1",
            )
            .bind(position_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PositionFacts {
                    id: row.try_get("id")?,
                    title: row.try_get("title")?,
                })
            })
            .transpose()
        })
    }

    fn policy<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<PolicyRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT version::bigint version,revision::bigint revision,user_filter_group_id \
                 FROM position_gpo_policies WHERE company_id=$1 AND position_id=$2",
            )
            .bind(company_id)
            .bind(position_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(PolicyRow {
                    version: row.try_get("version")?,
                    revision: row.try_get("revision")?,
                    user_filter_group_id: row.try_get("user_filter_group_id")?,
                })
            })
            .transpose()
        })
    }

    fn connection<'a>(
        &'a mut self,
        company_id: &'a str,
        connection_id: &'a str,
    ) -> GpoFuture<'a, Option<ConnectionRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT id,provider,config,status,version::bigint version FROM iam_connections \
                 WHERE id=$1 AND company_id=$2 AND status='active'",
            )
            .bind(connection_id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                let config: Value = row.try_get("config").unwrap_or(Value::Null);
                Ok::<_, Error>(ConnectionRow {
                    id: row.try_get("id")?,
                    provider: row.try_get("provider")?,
                    config,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn directory_state<'a>(
        &'a mut self,
        company_id: &'a str,
        connection_id: &'a str,
    ) -> GpoFuture<'a, Option<DirectoryState>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT version::bigint version,tested_version::bigint tested_version,write_enabled \
                 FROM people_directory_state WHERE company_id=$1 AND connection_id=$2",
            )
            .bind(company_id)
            .bind(connection_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(DirectoryState {
                    version: row.try_get("version")?,
                    tested_version: row.try_get("tested_version")?,
                    write_enabled: row.try_get("write_enabled")?,
                })
            })
            .transpose()
        })
    }

    fn holders<'a>(
        &'a mut self,
        connection_id: &'a str,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Vec<HolderRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT e.id engagement_id,e.person_id,e.version::bigint version,p.legal_name name,\
                 i.external_id account_id \
                 FROM hr_engagements e JOIN hr_people p \
                  ON p.id=e.person_id AND p.company_id=e.company_id \
                 LEFT JOIN LATERAL (SELECT DISTINCT ident.external_id FROM iam_members m \
                   JOIN iam_identities ident ON ident.id=m.identity_id \
                  WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active' \
                   AND ident.company_id=e.company_id AND ident.person_id=e.person_id \
                   AND ident.connection_id=$1 AND ident.enabled=true) i ON true \
                 WHERE e.company_id=$2 AND e.position_id=$3 AND e.status='active' \
                  AND e.start_date<=CURRENT_DATE::text \
                  AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) \
                  AND p.status='active' AND p.merged_into_id IS NULL \
                 ORDER BY p.legal_name,e.id,i.external_id",
            )
            .bind(connection_id)
            .bind(company_id)
            .bind(position_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(HolderRow {
                        engagement_id: row.try_get("engagement_id")?,
                        person_id: row.try_get("person_id")?,
                        version: row.try_get("version")?,
                        name: row.try_get("name")?,
                        account_id: row.try_get("account_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn shared_mappings<'a>(
        &'a mut self,
        company_id: &'a str,
        connection_id: &'a str,
        gpo_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Vec<Value>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows: Value = sqlx::query_scalar(
                "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
                  'position_id',p.position_id,'revision',p.revision,\
                  'user_filter_group_id',p.user_filter_group_id,\
                  'computer_target_ou_id',p.computer_target_ou_id) ORDER BY p.position_id),'[]'::jsonb) \
                 FROM position_gpo_policies p \
                 JOIN organization_positions o \
                  ON o.company_id=p.company_id AND o.id=p.position_id AND o.active=1 \
                 WHERE p.company_id=$1 AND p.connection_id=$2 AND p.gpo_id=$3 AND p.position_id<>$4",
            )
            .bind(company_id)
            .bind(connection_id)
            .bind(gpo_id)
            .bind(position_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(rows.as_array().cloned().unwrap_or_default())
        })
    }

    fn commit_publish<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a PublishPlan,
        request_hash: &'a str,
    ) -> GpoFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_catalog().await?;
            sqlx::query(
                "INSERT INTO position_gpo_revisions\
                 (id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,\
                 computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,\
                 reason,command_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11,$12,$13,$14)",
            )
            .bind(&plan.command_id)
            .bind(&plan.company_id)
            .bind(&plan.position_id)
            .bind(plan.revision as i32)
            .bind(&plan.connection_id)
            .bind(&plan.gpo_id)
            .bind(&plan.user_filter_group_id)
            .bind(&plan.computer_target_ou_id)
            .bind(&plan.directory_gpo_version)
            .bind(json!(plan.person_ids).to_string())
            .bind(&plan.user_id)
            .bind(&plan.reason)
            .bind(key)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let updated = sqlx::query(
                "INSERT INTO position_gpo_policies\
                 (company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,\
                 computer_target_ou_id,directory_config_status,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',$9) \
                 ON CONFLICT(company_id,position_id) DO UPDATE SET \
                  version=excluded.version,revision=excluded.revision,connection_id=excluded.connection_id,\
                  gpo_id=excluded.gpo_id,user_filter_group_id=excluded.user_filter_group_id,\
                  computer_target_ou_id=excluded.computer_target_ou_id,directory_config_status='pending',\
                  directory_config_error=NULL,updated_at=excluded.updated_at \
                 WHERE position_gpo_policies.version=$10",
            )
            .bind(&plan.company_id)
            .bind(&plan.position_id)
            .bind(plan.version as i32)
            .bind(plan.revision as i32)
            .bind(&plan.connection_id)
            .bind(&plan.gpo_id)
            .bind(&plan.user_filter_group_id)
            .bind(&plan.computer_target_ou_id)
            .bind(plan.now)
            .bind(plan.expected_version as i32)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(version_conflict().into());
            }
            self.effects(
                actor,
                key,
                request_hash,
                crate::domain::position_gpo::PUBLISH_OPERATION,
                &plan.position_id,
                plan.version,
                &plan.result,
                plan.now,
            )
            .await?;
            Ok(plan.result.clone())
        })
    }

    fn mapping<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<MappingRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT p.version::bigint version,p.revision::bigint revision,o.title,\
                 p.connection_id,p.gpo_id,p.user_filter_group_id,p.computer_target_ou_id \
                 FROM position_gpo_policies p JOIN organization_positions o \
                  ON o.id=p.position_id AND o.company_id=p.company_id \
                 WHERE p.company_id=$1 AND p.position_id=$2",
            )
            .bind(company_id)
            .bind(position_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(MappingRow {
                    version: row.try_get("version")?,
                    revision: row.try_get("revision")?,
                    title: row.try_get("title")?,
                    connection_id: row.try_get("connection_id")?,
                    gpo_id: row.try_get("gpo_id")?,
                    user_filter_group_id: row.try_get("user_filter_group_id")?,
                    computer_target_ou_id: row.try_get("computer_target_ou_id")?,
                })
            })
            .transpose()
        })
    }

    fn unmap_holders<'a>(
        &'a mut self,
        connection_id: &'a str,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Vec<Value>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows: Value = sqlx::query_scalar(
                "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
                  'person_id',e.person_id,'legal_name',p.legal_name,\
                  'account_ids',COALESCE((SELECT jsonb_agg(ident.external_id ORDER BY ident.external_id) \
                   FROM iam_members m JOIN iam_identities ident ON ident.id=m.identity_id \
                   WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active' \
                    AND ident.company_id=e.company_id AND ident.person_id=e.person_id \
                    AND ident.connection_id=$1 AND ident.enabled=true),'[]'::jsonb)) \
                  ORDER BY p.legal_name,e.id),'[]'::jsonb) \
                 FROM hr_engagements e JOIN hr_people p \
                  ON p.id=e.person_id AND p.company_id=e.company_id \
                 WHERE e.company_id=$2 AND e.position_id=$3 AND e.status='active' \
                  AND e.start_date<=CURRENT_DATE::text \
                  AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) \
                  AND p.status='active' AND p.merged_into_id IS NULL",
            )
            .bind(connection_id)
            .bind(company_id)
            .bind(position_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(rows.as_array().cloned().unwrap_or_default())
        })
    }

    fn commit_unmap<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a UnmapPlan,
        request_hash: &'a str,
    ) -> GpoFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_catalog().await?;
            sqlx::query(
                "INSERT INTO position_gpo_revisions\
                 (id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,\
                 computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,\
                 reason,command_id,created_at) \
                 VALUES($1,$2,$3,$4,NULL,NULL,NULL,NULL,NULL,$5::jsonb,$6,$7,$8,$9)",
            )
            .bind(&plan.command_id)
            .bind(&plan.company_id)
            .bind(&plan.position_id)
            .bind(plan.revision as i32)
            .bind(json!(plan.person_ids).to_string())
            .bind(&plan.user_id)
            .bind(&plan.reason)
            .bind(key)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let updated = sqlx::query(
                "UPDATE position_gpo_policies SET version=$1,revision=$2,connection_id=NULL,\
                 gpo_id=NULL,user_filter_group_id=NULL,computer_target_ou_id=NULL,\
                 directory_config_status='directory_applied',directory_config_error=NULL,updated_at=$3 \
                 WHERE company_id=$4 AND position_id=$5 AND version=$6",
            )
            .bind(plan.version as i32)
            .bind(plan.revision as i32)
            .bind(plan.now)
            .bind(&plan.company_id)
            .bind(&plan.position_id)
            .bind(plan.expected_version as i32)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(crate::domain::position_gpo::mapping_changed().into());
            }
            self.effects(
                actor,
                key,
                request_hash,
                crate::domain::position_gpo::UNMAP_OPERATION,
                &plan.position_id,
                plan.version,
                &plan.result,
                plan.now,
            )
            .await?;
            Ok(plan.result.clone())
        })
    }

    fn overview<'a>(&'a mut self, company_id: &'a str) -> GpoFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let policies: Value = sqlx::query_scalar(
                "SELECT COALESCE(jsonb_agg(to_jsonb(p) || jsonb_build_object('mapped',p.connection_id IS NOT NULL,'title',o.title) ORDER BY o.title),'[]'::jsonb) \
                 FROM position_gpo_policies p JOIN organization_positions o \
                  ON o.id=p.position_id AND o.company_id=p.company_id \
                 WHERE p.company_id=$1",
            )
            .bind(company_id)
            .fetch_one(&mut *self.connection)
            .await?;
            let jobs: Value = sqlx::query_scalar(
                "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
                  'id',j.id,'position_id',j.position_id,'person_id',j.person_id,\
                  'engagement_id',j.engagement_id,'account_id',j.account_id,'legal_name',p.legal_name,\
                  'revision',j.revision,'desired_action',j.desired_action,'status',j.status,\
                  'version',j.version,'attempts',j.attempts,'last_error',j.last_error,\
                  'directory_observed_at',j.directory_observed_at,\
                  'endpoint_observed_at',j.endpoint_observed_at) ORDER BY j.updated_at DESC),'[]'::jsonb) \
                 FROM (SELECT * FROM position_gpo_jobs WHERE company_id=$1 \
                  ORDER BY updated_at DESC LIMIT 200) j \
                 JOIN hr_people p ON p.id=j.person_id",
            )
            .bind(company_id)
            .fetch_one(&mut *self.connection)
            .await?;
            let positions: Value = sqlx::query_scalar(
                "SELECT COALESCE(jsonb_agg(jsonb_build_object(\
                  'id',o.id,'title',o.title,'holder_count',count(DISTINCT e.id)::integer,\
                  'mapping_version',p.version,'mapped',p.connection_id IS NOT NULL,\
                  'mapping_status',CASE WHEN p.connection_id IS NOT NULL \
                   THEN p.directory_config_status ELSE NULL END) ORDER BY o.title,o.id),'[]'::jsonb) \
                 FROM organization_positions o \
                 LEFT JOIN position_gpo_policies p \
                  ON p.company_id=o.company_id AND p.position_id=o.id \
                 LEFT JOIN hr_engagements e ON e.company_id=o.company_id AND e.position_id=o.id \
                  AND e.status='active' AND e.start_date<=CURRENT_DATE::text \
                  AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) \
                 WHERE o.company_id=$1 AND o.active=1 \
                 GROUP BY o.id,o.title,p.version,p.connection_id,p.directory_config_status",
            )
            .bind(company_id)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok(json!({"policies": policies, "jobs": jobs, "positions": positions}))
        })
    }

    fn job_state<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> GpoFuture<'a, Option<JobStateRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT version::bigint version,status FROM position_gpo_jobs \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(JobStateRow {
                    version: row.try_get("version")?,
                    status: row.try_get("status")?,
                })
            })
            .transpose()
        })
    }

    fn commit_job_retry<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        input: &'a JobRetryInput,
        request_hash: &'a str,
        now: i64,
    ) -> GpoFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let updated = sqlx::query(
                "UPDATE position_gpo_jobs SET status='queued',version=version+1,last_error=NULL,\
                 updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4 \
                 AND status IN ('blocked','retry')",
            )
            .bind(now)
            .bind(&input.id)
            .bind(actor.company_id.as_str())
            .bind(input.expected_version as i32)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(crate::domain::position_gpo::job_changed().into());
            }
            let result = json!({
                "id": input.id,
                "status": "queued",
                "version": input.expected_version + 1,
            });
            self.effects(
                actor,
                key,
                request_hash,
                crate::domain::position_gpo::JOB_RETRY_OPERATION,
                &input.id,
                input.expected_version + 1,
                &result,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn config<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
    ) -> GpoFuture<'a, Option<ConfigRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT version::bigint version,revision::bigint revision,directory_config_status \
                 FROM position_gpo_policies WHERE company_id=$1 AND position_id=$2",
            )
            .bind(company_id)
            .bind(position_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(ConfigRow {
                    version: row.try_get("version")?,
                    revision: row.try_get("revision")?,
                    directory_config_status: row.try_get("directory_config_status")?,
                })
            })
            .transpose()
        })
    }

    fn commit_config_retry<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        input: &'a ConfigRetryInput,
        request_hash: &'a str,
        now: i64,
    ) -> GpoFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let updated = sqlx::query(
                "UPDATE position_gpo_policies SET directory_config_status='pending',\
                 directory_config_error=NULL,updated_at=$1 \
                 WHERE company_id=$2 AND position_id=$3 AND version=$4 \
                 AND directory_config_status='blocked'",
            )
            .bind(now)
            .bind(actor.company_id.as_str())
            .bind(&input.position_id)
            .bind(input.expected_version as i32)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(crate::domain::position_gpo::config_changed().into());
            }
            let result = json!({
                "positionId": input.position_id,
                "version": input.expected_version,
                "status": "pending",
            });
            self.effects(
                actor,
                key,
                request_hash,
                crate::domain::position_gpo::CONFIG_RETRY_OPERATION,
                &input.position_id,
                input.expected_version,
                &result,
                now,
            )
            .await?;
            Ok(result)
        })
    }

    fn effective_rules<'a>(&'a mut self) -> GpoFuture<'a, Vec<EffectiveRule>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT r.id,r.company_id,r.revision::bigint revision,r.connection_id,r.gpo_id,\
                 r.user_filter_group_id,r.computer_target_ou_id,r.legacy_position_id \
                 FROM organization_positions p JOIN organization_gpo_rules r \
                  ON r.company_id=p.company_id AND r.active \
                  AND r.target_kind='position' AND r.target_id=p.id \
                 WHERE p.active=1 ORDER BY p.id,r.id",
            )
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(EffectiveRule {
                        id: row.try_get("id")?,
                        company_id: row.try_get("company_id")?,
                        revision: row.try_get("revision")?,
                        connection_id: row.try_get("connection_id")?,
                        gpo_id: row.try_get("gpo_id")?,
                        user_filter_group_id: row.try_get("user_filter_group_id")?,
                        computer_target_ou_id: row.try_get("computer_target_ou_id")?,
                        legacy_position_id: row.try_get("legacy_position_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn config_ownership<'a>(&'a mut self) -> GpoFuture<'a, Vec<OwnershipRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT connection_id,gpo_id,kind,target_id,status \
                 FROM position_gpo_config_ownership",
            )
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(OwnershipRow {
                        connection_id: row.try_get("connection_id")?,
                        gpo_id: row.try_get("gpo_id")?,
                        kind: row.try_get("kind")?,
                        target_id: row.try_get("target_id")?,
                        status: row.try_get("status")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn connection_company<'a>(
        &'a mut self,
        connection_id: &'a str,
    ) -> GpoFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT company_id FROM iam_connections WHERE id=$1")
                    .bind(connection_id)
                    .fetch_optional(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn set_ownership<'a>(
        &'a mut self,
        kind: &'a str,
        connection_id: &'a str,
        gpo_id: &'a str,
        target_id: &'a str,
        status: Option<&'a str>,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            match status {
                None => {
                    sqlx::query(
                        "DELETE FROM position_gpo_config_ownership WHERE connection_id=$1 \
                         AND gpo_id=$2 AND kind=$3 AND target_id=$4",
                    )
                    .bind(connection_id)
                    .bind(gpo_id)
                    .bind(kind)
                    .bind(target_id)
                    .execute(&mut *self.connection)
                    .await?;
                }
                Some("pending") => {
                    sqlx::query(
                        "INSERT INTO position_gpo_config_ownership\
                         (connection_id,gpo_id,kind,target_id,created_at,status) \
                         VALUES($1,$2,$3,$4,$5,'pending') ON CONFLICT DO NOTHING",
                    )
                    .bind(connection_id)
                    .bind(gpo_id)
                    .bind(kind)
                    .bind(target_id)
                    .bind(now)
                    .execute(&mut *self.connection)
                    .await?;
                }
                Some("written") => {
                    sqlx::query(
                        "UPDATE position_gpo_config_ownership SET status='written' \
                         WHERE connection_id=$1 AND gpo_id=$2 AND kind=$3 AND target_id=$4 \
                         AND status='pending'",
                    )
                    .bind(connection_id)
                    .bind(gpo_id)
                    .bind(kind)
                    .bind(target_id)
                    .execute(&mut *self.connection)
                    .await?;
                }
                Some("confirmed") => {
                    sqlx::query(
                        "UPDATE position_gpo_config_ownership SET status='confirmed' \
                         WHERE connection_id=$1 AND gpo_id=$2 AND kind=$3 AND target_id=$4 \
                         AND status='written'",
                    )
                    .bind(connection_id)
                    .bind(gpo_id)
                    .bind(kind)
                    .bind(target_id)
                    .execute(&mut *self.connection)
                    .await?;
                }
                Some(_) => return Err(source_invalid().into()),
            }
            Ok(())
        })
    }

    fn mark_rule<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
        revision: i64,
        status: &'a str,
        error: Option<&'a str>,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "UPDATE organization_gpo_rules SET directory_config_status=$4,\
                 directory_config_error=$5 WHERE company_id=$1 AND id=$2 AND revision=$3",
            )
            .bind(company_id)
            .bind(id)
            .bind(revision as i32)
            .bind(status)
            .bind(error)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn mark_legacy<'a>(
        &'a mut self,
        company_id: &'a str,
        position_id: &'a str,
        revision: i64,
        status: &'a str,
        error: Option<&'a str>,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "UPDATE position_gpo_policies SET directory_config_status=$4,\
                 directory_config_error=$5 WHERE company_id=$1 AND position_id=$2 AND revision=$3",
            )
            .bind(company_id)
            .bind(position_id)
            .bind(revision as i32)
            .bind(status)
            .bind(error)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn desired_facts<'a>(&'a mut self) -> GpoFuture<'a, Vec<DesiredRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT e.id engagement_id,e.company_id,e.position_id,e.person_id,\
                 e.version::bigint engagement_version,r.id rule_id,r.legacy_position_id,\
                 r.revision::bigint revision,r.connection_id,r.gpo_id,\
                 r.user_filter_group_id filter_group_id,r.computer_target_ou_id,\
                 i.external_id account_id \
                 FROM hr_engagements e JOIN hr_people person \
                  ON person.id=e.person_id AND person.company_id=e.company_id \
                 JOIN organization_positions position \
                  ON position.id=e.position_id AND position.company_id=e.company_id AND position.active=1 \
                 JOIN organization_gpo_rules r ON r.company_id=e.company_id AND r.active \
                  AND r.target_kind='position' AND r.target_id=position.id \
                 LEFT JOIN LATERAL (SELECT DISTINCT ident.external_id FROM iam_members m \
                   JOIN iam_identities ident ON ident.id=m.identity_id \
                  WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active' \
                   AND ident.company_id=e.company_id AND ident.person_id=e.person_id \
                   AND ident.connection_id=r.connection_id AND ident.enabled=true) i ON true \
                 WHERE e.status='active' AND e.start_date<=CURRENT_DATE::text \
                  AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) \
                  AND person.status='active' AND person.merged_into_id IS NULL",
            )
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(DesiredRow {
                        engagement_id: row.try_get("engagement_id")?,
                        company_id: row.try_get("company_id")?,
                        position_id: row.try_get("position_id")?,
                        person_id: row.try_get("person_id")?,
                        engagement_version: row.try_get("engagement_version")?,
                        rule_id: row.try_get("rule_id")?,
                        legacy_position_id: row.try_get("legacy_position_id")?,
                        revision: row.try_get("revision")?,
                        connection_id: row.try_get("connection_id")?,
                        gpo_id: row.try_get("gpo_id")?,
                        filter_group_id: row.try_get("filter_group_id")?,
                        computer_target_ou_id: row.try_get("computer_target_ou_id")?,
                        account_id: row.try_get("account_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn claims<'a>(&'a mut self) -> GpoFuture<'a, Vec<ClaimRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT c.connection_id,c.group_id,c.member_id,c.source_id,c.rule_id,\
                 c.company_id,c.position_id,c.person_id,c.engagement_id,\
                 c.engagement_version::bigint engagement_version,c.revision::bigint revision,\
                 c.gpo_id,c.computer_target_ou_id \
                 FROM organization_gpo_membership_claims c \
                 JOIN people_directory_membership_sources s \
                  ON s.connection_id=c.connection_id AND s.group_id=c.group_id \
                  AND s.member_id=c.member_id AND s.source_id=c.source_id",
            )
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(ClaimRow {
                        connection_id: row.try_get("connection_id")?,
                        group_id: row.try_get("group_id")?,
                        member_id: row.try_get("member_id")?,
                        source_id: row.try_get("source_id")?,
                        rule_id: row.try_get("rule_id")?,
                        company_id: row.try_get("company_id")?,
                        position_id: row.try_get("position_id")?,
                        person_id: row.try_get("person_id")?,
                        engagement_id: row.try_get("engagement_id")?,
                        engagement_version: row.try_get("engagement_version")?,
                        revision: row.try_get("revision")?,
                        gpo_id: row.try_get("gpo_id")?,
                        computer_target_ou_id: row.try_get("computer_target_ou_id")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn pending_adds<'a>(&'a mut self) -> GpoFuture<'a, Vec<JobRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(AssertSqlSafe(format!(
                "SELECT {JOB_COLUMNS} FROM position_gpo_jobs \
                 WHERE desired_action='add' AND status IN ('queued','retry','blocked')"
            )))
            .fetch_all(&mut *self.connection)
            .await?;
            collect_jobs(rows).await
        })
    }

    fn waiting_identities<'a>(&'a mut self) -> GpoFuture<'a, Vec<JobRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(AssertSqlSafe(format!(
                "SELECT {JOB_COLUMNS} FROM position_gpo_jobs WHERE status='awaiting_identity'"
            )))
            .fetch_all(&mut *self.connection)
            .await?;
            collect_jobs(rows).await
        })
    }

    fn latest_delivery<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        desired_action: &'a str,
    ) -> GpoFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT status FROM position_gpo_jobs WHERE company_id=$1 AND position_id=$2 \
                 AND source_id=$3 AND account_id=$4 AND rule_id=$5 AND revision=$6 \
                 AND connection_id=$7 AND filter_group_id=$8 AND desired_action=$9 \
                 AND status IN ('queued','running','retry','blocked','directory_applied') \
                 ORDER BY updated_at DESC LIMIT 1",
            )
            .bind(&row.company_id)
            .bind(&row.position_id)
            .bind(row.source_id())
            .bind(&row.account_id)
            .bind(&row.rule_id)
            .bind(row.revision as i32)
            .bind(&row.connection_id)
            .bind(&row.filter_group_id)
            .bind(desired_action)
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn owned_membership<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        source_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM people_directory_membership_sources \
                 WHERE connection_id=$1 AND group_id=$2 AND member_id=$3 AND source_id=$4)",
            )
            .bind(&row.connection_id)
            .bind(&row.filter_group_id)
            .bind(&row.account_id)
            .bind(source_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn enqueue<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        id: &'a str,
        desired_action: &'a str,
        status: &'a str,
        source_id: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO position_gpo_jobs(id,company_id,position_id,person_id,engagement_id,\
                 account_id,revision,engagement_version,connection_id,gpo_id,filter_group_id,\
                 computer_target_ou_id,desired_action,status,created_at,updated_at,rule_id,source_id) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15,$16,$17) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(&row.company_id)
            .bind(&row.position_id)
            .bind(&row.person_id)
            .bind(&row.engagement_id)
            .bind(&row.account_id)
            .bind(row.revision as i32)
            .bind(row.engagement_version as i32)
            .bind(&row.connection_id)
            .bind(&row.gpo_id)
            .bind(&row.filter_group_id)
            .bind(&row.computer_target_ou_id)
            .bind(desired_action)
            .bind(status)
            .bind(now)
            .bind(&row.rule_id)
            .bind(source_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn supersede<'a>(
        &'a mut self,
        id: &'a str,
        from_status: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = if from_status == "awaiting_identity" {
                "UPDATE position_gpo_jobs SET status='superseded',version=version+1,updated_at=$1 \
                 WHERE id=$2 AND status='awaiting_identity'"
            } else {
                "UPDATE position_gpo_jobs SET status='superseded',version=version+1,updated_at=$1 \
                 WHERE id=$2 AND status IN ('queued','retry','blocked')"
            };
            sqlx::query(sql)
                .bind(now)
                .bind(id)
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn recover_interrupted<'a>(
        &'a mut self,
        now: i64,
        stale_before: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "UPDATE position_gpo_jobs SET status='retry',version=version+1,\
                 last_error='WORKER_INTERRUPTED',updated_at=$1 \
                 WHERE status='running' AND updated_at<$2",
            )
            .bind(now)
            .bind(stale_before)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn due_deliveries<'a>(&'a mut self, limit: i64) -> GpoFuture<'a, Vec<JobRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(AssertSqlSafe(format!(
                "SELECT {JOB_COLUMNS} FROM position_gpo_jobs \
                 WHERE status IN ('queued','retry') ORDER BY updated_at,id LIMIT $1"
            )))
            .bind(limit)
            .fetch_all(&mut *self.connection)
            .await?;
            collect_jobs(rows).await
        })
    }

    fn claim_delivery<'a>(
        &'a mut self,
        id: &'a str,
        version: i64,
        now: i64,
    ) -> GpoFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let claimed: Option<String> = sqlx::query_scalar(
                "UPDATE position_gpo_jobs SET status='running',attempts=attempts+1,\
                 version=version+1,updated_at=$1 WHERE id=$2 AND version=$3 \
                 AND status IN ('queued','retry') RETURNING id",
            )
            .bind(now)
            .bind(id)
            .bind(version as i32)
            .fetch_optional(&mut *self.connection)
            .await?;
            Ok(claimed.is_some())
        })
    }

    fn finish_delivery<'a>(
        &'a mut self,
        id: &'a str,
        status: &'a str,
        observed_at: Option<i64>,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "UPDATE position_gpo_jobs SET status=$1,\
                 directory_observed_at=COALESCE($2,directory_observed_at),last_error=NULL,\
                 version=version+1,updated_at=$3 WHERE id=$4",
            )
            .bind(status)
            .bind(observed_at)
            .bind(now)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn block_delivery<'a>(
        &'a mut self,
        id: &'a str,
        error: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "UPDATE position_gpo_jobs SET status='blocked',last_error=$1,\
                 version=version+1,updated_at=$2 WHERE id=$3",
            )
            .bind(error)
            .bind(now)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn defer_replacement<'a>(
        &'a mut self,
        id: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "UPDATE position_gpo_jobs SET status='retry',\
                 last_error='AWAITING_REPLACEMENT_SOURCE',version=version+1,updated_at=$1 \
                 WHERE id=$2",
            )
            .bind(now)
            .bind(id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn membership<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        source_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error> {
        self.owned_membership(row, source_id)
    }

    fn existing_membership<'a>(
        &'a mut self,
        connection_id: &'a str,
        group_id: &'a str,
        member_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM people_directory_membership_sources \
                 WHERE connection_id=$1 AND group_id=$2 AND member_id=$3)",
            )
            .bind(connection_id)
            .bind(group_id)
            .bind(member_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn other_membership<'a>(
        &'a mut self,
        row: &'a DesiredRow,
        source_id: &'a str,
    ) -> GpoFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM people_directory_membership_sources \
                 WHERE connection_id=$1 AND group_id=$2 AND member_id=$3 AND source_id<>$4)",
            )
            .bind(&row.connection_id)
            .bind(&row.filter_group_id)
            .bind(&row.account_id)
            .bind(source_id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn preserve_external<'a>(
        &'a mut self,
        connection_id: &'a str,
        group_id: &'a str,
        member_id: &'a str,
        now: i64,
    ) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO people_directory_membership_sources\
                 (connection_id,group_id,member_id,source_id,created_at) \
                 VALUES($1,$2,$3,'external',$4) ON CONFLICT DO NOTHING",
            )
            .bind(connection_id)
            .bind(group_id)
            .bind(member_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn record_claim<'a>(&'a mut self, job: &'a JobRow, now: i64) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "INSERT INTO people_directory_membership_sources\
                 (connection_id,group_id,member_id,source_id,created_at) \
                 VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
            )
            .bind(&job.connection_id)
            .bind(&job.filter_group_id)
            .bind(&job.account_id)
            .bind(&job.source_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "INSERT INTO organization_gpo_membership_claims\
                 (connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,\
                 engagement_id,person_id,revision,engagement_version,gpo_id,computer_target_ou_id) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
                 ON CONFLICT(connection_id,group_id,member_id,source_id) DO UPDATE SET \
                  rule_id=excluded.rule_id,position_id=excluded.position_id,\
                  revision=excluded.revision,engagement_version=excluded.engagement_version,\
                  gpo_id=excluded.gpo_id,computer_target_ou_id=excluded.computer_target_ou_id",
            )
            .bind(&job.connection_id)
            .bind(&job.filter_group_id)
            .bind(&job.account_id)
            .bind(&job.source_id)
            .bind(&job.rule_id)
            .bind(&job.company_id)
            .bind(&job.position_id)
            .bind(&job.engagement_id)
            .bind(&job.person_id)
            .bind(job.revision as i32)
            .bind(job.engagement_version as i32)
            .bind(&job.gpo_id)
            .bind(&job.computer_target_ou_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn delete_claim<'a>(&'a mut self, job: &'a JobRow) -> GpoFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query(
                "DELETE FROM people_directory_membership_sources WHERE connection_id=$1 \
                 AND group_id=$2 AND member_id=$3 AND source_id=$4",
            )
            .bind(&job.connection_id)
            .bind(&job.filter_group_id)
            .bind(&job.account_id)
            .bind(&job.source_id)
            .execute(&mut *self.connection)
            .await?;
            sqlx::query(
                "DELETE FROM organization_gpo_membership_claims WHERE connection_id=$1 \
                 AND group_id=$2 AND member_id=$3 AND source_id=$4",
            )
            .bind(&job.connection_id)
            .bind(&job.filter_group_id)
            .bind(&job.account_id)
            .bind(&job.source_id)
            .execute(&mut *self.connection)
            .await?;
            Ok(())
        })
    }

    fn delivery_rule<'a>(
        &'a mut self,
        company_id: &'a str,
        rule_id: &'a str,
        revision: i64,
    ) -> GpoFuture<'a, Option<RuleStatusRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT directory_config_status FROM organization_gpo_rules \
                 WHERE company_id=$1 AND id=$2 AND revision=$3 AND active",
            )
            .bind(company_id)
            .bind(rule_id)
            .bind(revision as i32)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(RuleStatusRow {
                    directory_config_status: row.try_get("directory_config_status")?,
                })
            })
            .transpose()
        })
    }
}

async fn collect_jobs(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<Vec<JobRow>, PgTransactionError<BusinessError>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(JobRow {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            position_id: row.try_get("position_id")?,
            person_id: row.try_get("person_id")?,
            engagement_id: row.try_get("engagement_id")?,
            account_id: row.try_get("account_id")?,
            revision: row.try_get("revision")?,
            engagement_version: row.try_get("engagement_version")?,
            connection_id: row.try_get("connection_id")?,
            gpo_id: row.try_get("gpo_id")?,
            filter_group_id: row.try_get("filter_group_id")?,
            computer_target_ou_id: row.try_get("computer_target_ou_id")?,
            desired_action: row.try_get("desired_action")?,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            rule_id: row.try_get("rule_id")?,
            source_id: row.try_get("source_id")?,
        });
    }
    Ok(out)
}
