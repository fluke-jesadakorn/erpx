//! PostgreSQL adapter for native Talent candidate creation and privacy.
//!
//! SQL is Talent-owned (`talent_candidates`, `talent_candidate_events`,
//! `talent_candidate_privacy_events`) plus the shared command receipt, audit and
//! outbox tables and the read-only IAM authority functions. The caller's
//! serializable Holding transaction is required and no commit is performed here.
use crate::application::candidate::{
    CandidateFuture, TalentCandidateCommandRepository, TalentCandidateReadRepository,
};
use crate::domain::candidate::{
    CREATE_OPERATION, CandidateReceipt, CandidateRow, CreatePlan, PrivacyEventRow, PrivacyPlan,
    TalentActor, idempotency_conflict, readback_invalid,
};
use crate::domain::receipt::ReceiptSnapshot;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'records.read') \
 AND iam_role_allows(a.role,a.company_id,'records.review')";

const SELECT: &str = "SELECT c.id,c.company_id,c.legal_name,c.email,c.phone,c.consent_evidence,\
 c.retention_until,c.status,c.privacy_hold,c.privacy_requested_by,\
 CASE WHEN c.privacy_requested_by IS NULL THEN false \
 ELSE iam_same_person($2,c.privacy_requested_by,$3) END requested_by_self,\
 c.created_by,c.version,c.created_at,c.updated_at FROM talent_candidates c";

fn map_row(row: sqlx::postgres::PgRow) -> Result<CandidateRow, sqlx::Error> {
    Ok(CandidateRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        legal_name: row.try_get("legal_name")?,
        email: row.try_get("email")?,
        phone: row.try_get("phone")?,
        consent_evidence: row.try_get("consent_evidence")?,
        retention_until: row.try_get("retention_until")?,
        status: row.try_get("status")?,
        privacy_hold: row.try_get("privacy_hold")?,
        privacy_requested_by: row.try_get("privacy_requested_by")?,
        requested_by_self: row.try_get("requested_by_self")?,
        created_by: row.try_get("created_by")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub struct PgTalentCandidates<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgTalentCandidates<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("TALENT_CANDIDATE_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("TALENT_CANDIDATE_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("talent-candidate-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn current(&mut self, actor: &TalentActor, write: bool, now: i64) -> Result<bool, Error> {
        self.ready().await?;
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE {AUTHORITY} \
             AND ($5::text IS NULL OR a.role=$5::text) \
             AND (NOT $6::boolean OR iam_role_allows(a.role,a.company_id,'records.write')))"
        );
        Ok(sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(actor.role.as_deref())
            .bind(write)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn current_privacy(
        &mut self,
        actor: &TalentActor,
        permission: Option<&str>,
        now: i64,
    ) -> Result<bool, Error> {
        self.ready().await?;
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE a.id=$1 AND a.user_id=$2 \
             AND a.company_id=$3 AND a.revoked_at IS NULL \
             AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
             AND ($5::text IS NULL OR a.role=$5::text) \
             AND ({}) )",
            if permission.is_some() {
                "iam_role_allows(a.role,a.company_id,$6)"
            } else {
                "iam_role_allows(a.role,a.company_id,'legal.hold') \
                 OR iam_role_allows(a.role,a.company_id,'legal.hold.release') \
                 OR iam_role_allows(a.role,a.company_id,'legal.destroy') \
                 OR iam_role_allows(a.role,a.company_id,'people.manage')"
            }
        );
        let mut query = sqlx::query_scalar::<_, bool>(AssertSqlSafe(sql))
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(actor.role.as_deref());
        if let Some(permission) = permission {
            query = query.bind(permission);
        }
        Ok(query.fetch_one(&mut *self.connection).await?)
    }

    async fn row(
        &mut self,
        actor: &TalentActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<CandidateRow>, Error> {
        self.ready().await?;
        let sql = format!(
            "{SELECT} WHERE c.id=$1 AND c.company_id=$3{}",
            if lock { " FOR UPDATE OF c" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_row).transpose().map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        result: &CandidateReceipt,
        now: i64,
    ) -> Result<(), Error> {
        let changed = sqlx::query(
            "INSERT INTO command_receipts\
             (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(key)
        .bind(operation)
        .bind(actor.user_id.as_str())
        .bind(request_hash)
        .bind(record_id)
        .bind(result.to_wire().to_string())
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if changed != 1 {
            return Err(idempotency_conflict().into());
        }
        Ok(())
    }

    async fn audit(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_id: &str,
        version: i64,
        now: i64,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO audit_events\
             (id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,\
             purpose,created_at,authority_json) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'talent_candidate',$5,$6,$4,$7,$8)",
        )
        .bind(key)
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn outbox(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_id: &str,
        status: &str,
        version: i64,
        privacy_hold: bool,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": record_id,
            "state": status,
            "companyId": actor.company_id.as_str(),
            "branchId": actor.branch_id.as_ref().map(|value| value.as_str()),
            "departmentId": actor.department_id.as_ref().map(|value| value.as_str()),
            "privacyHold": privacy_hold,
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'talent_candidate',$3,$4,$5,$6)",
        )
        .bind(key)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(payload)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn readback_create(
        &mut self,
        actor: &TalentActor,
        plan: &CreatePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.id != plan.id
            || row.company_id != plan.company_id
            || row.legal_name != plan.legal_name
            || row.email != plan.email
            || row.phone != plan.phone
            || row.consent_evidence != plan.consent_evidence
            || row.retention_until != plan.retention_until
            || row.status != "active"
            || row.privacy_hold
            || row.privacy_requested_by.is_some()
            || row.created_by != plan.created_by
            || row.version != 1
            || row.created_at != plan.now
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_privacy(
        &mut self,
        actor: &TalentActor,
        plan: &PrivacyPlan,
    ) -> Result<(), Error> {
        let Some(row) = self.row(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        let erased_name = "Erased recruitment contact";
        if row.status != plan.status
            || row.version != plan.version
            || row.privacy_hold != plan.privacy_hold
            || row.privacy_requested_by != plan.privacy_requested_by
            || row.updated_at != plan.now
            || (plan.action == crate::domain::candidate::PrivacyAction::Erase
                && (row.legal_name != erased_name
                    || row.email != plan.erased_email
                    || row.phone.is_some()))
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }
}

impl TalentCandidateCommandRepository for PgTalentCandidates<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn current_privacy_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        permission: &'a str,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current_privacy(actor, Some(permission), now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> CandidateFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *self.connection)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CandidateFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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
                Ok::<_, Error>(ReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                    authority_json: row.try_get("authority_json")?,
                })
            })
            .transpose()
        })
    }

    fn candidate_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> CandidateFuture<'a, Option<CandidateRow>, Self::Error> {
        Box::pin(async move { self.row(actor, id, true).await })
    }

    fn commit_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreatePlan,
        request_hash: &'a str,
    ) -> CandidateFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO talent_candidates\
                 (id,company_id,legal_name,email,phone,consent_evidence,consented_at,\
                 retention_until,status,created_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,'active',$9,1,$10,$10) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.legal_name)
            .bind(&plan.email)
            .bind(&plan.phone)
            .bind(&plan.consent_evidence)
            .bind(plan.now)
            .bind(&plan.retention_until)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_candidate_events\
                 (id,candidate_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,1,'consent',$3,'Candidate consent recorded','active',$4) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(actor.user_id.as_str())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = CandidateReceipt::new(plan.id.clone(), "active", 1);
            self.insert_receipt(
                actor,
                key,
                CREATE_OPERATION,
                &plan.id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(actor, key, CREATE_OPERATION, &plan.id, 1, plan.now)
                .await?;
            self.outbox(
                actor,
                key,
                CREATE_OPERATION,
                &plan.id,
                "active",
                1,
                false,
                plan.now,
            )
            .await?;
            self.readback_create(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn commit_privacy<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a PrivacyPlan,
        request_hash: &'a str,
    ) -> CandidateFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = if plan.action == crate::domain::candidate::PrivacyAction::Erase {
                sqlx::query(
                    "UPDATE talent_candidates SET legal_name='Erased recruitment contact',email=$2,\
                     phone=NULL,consent_evidence='Recruitment contact erased after independent Legal review',\
                     status=$3,version=$4,updated_at=$5 \
                     WHERE id=$1 AND version=$6 AND status=$7 AND privacy_hold=$8",
                )
                .bind(&plan.id)
                .bind(&plan.erased_email)
                .bind(&plan.status)
                .bind(plan.version)
                .bind(plan.now)
                .bind(plan.expected_version)
                .bind(&plan.expected_status)
                .bind(plan.expected_hold)
                .execute(&mut *self.connection)
                .await?
                .rows_affected()
            } else {
                sqlx::query(
                    "UPDATE talent_candidates SET status=$2,privacy_hold=$3,privacy_requested_by=$4,\
                     version=$5,updated_at=$6 \
                     WHERE id=$1 AND version=$7 AND status=$8 AND privacy_hold=$9",
                )
                .bind(&plan.id)
                .bind(&plan.status)
                .bind(plan.privacy_hold)
                .bind(&plan.privacy_requested_by)
                .bind(plan.version)
                .bind(plan.now)
                .bind(plan.expected_version)
                .bind(&plan.expected_status)
                .bind(plan.expected_hold)
                .execute(&mut *self.connection)
                .await?
                .rows_affected()
            };
            if changed != 1 {
                return Err(crate::domain::candidate::candidate_changed().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_candidate_privacy_events\
                 (id,candidate_id,company_id,action,candidate_version,actor_id,reason,command_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.action.as_str())
            .bind(plan.version)
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(key)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            let receipt = CandidateReceipt::privacy(
                plan.id.clone(),
                &plan.status,
                plan.version,
                plan.privacy_hold,
            );
            let operation = plan.action.operation();
            self.insert_receipt(
                actor,
                key,
                &operation,
                &plan.id,
                request_hash,
                &receipt,
                plan.now,
            )
            .await?;
            self.audit(actor, key, &operation, &plan.id, plan.version, plan.now)
                .await?;
            self.outbox(
                actor,
                key,
                &operation,
                &plan.id,
                &plan.status,
                plan.version,
                plan.privacy_hold,
                plan.now,
            )
            .await?;
            self.readback_privacy(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl TalentCandidateReadRepository for PgTalentCandidates<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn current_privacy_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        now: i64,
    ) -> CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current_privacy(actor, None, now).await })
    }

    fn candidate<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> CandidateFuture<'a, Option<CandidateRow>, Self::Error> {
        Box::pin(async move { self.row(actor, id, false).await })
    }

    fn privacy_events<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> CandidateFuture<'a, Vec<PrivacyEventRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,action,candidate_version,created_at \
                 FROM talent_candidate_privacy_events \
                 WHERE candidate_id=$1 AND company_id=$2 ORDER BY candidate_version,id LIMIT 500",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(PrivacyEventRow {
                        id: row.try_get("id")?,
                        action: row.try_get("action")?,
                        candidate_version: row.try_get("candidate_version")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }
}
