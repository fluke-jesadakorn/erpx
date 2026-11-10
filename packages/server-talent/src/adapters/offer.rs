//! PostgreSQL adapter for the native Talent offer lifecycle and hire handoff.
//!
//! SQL is Talent-owned (`talent_offers`, `talent_offer_events`,
//! `talent_hire_handoffs`) plus read-only application/requisition/candidate and
//! IAM facts, the immutable People consumption receipt (read only) and the
//! shared command receipt, audit and outbox tables. The Finance Money snapshot
//! is a composed producer port: absent a producer every offer create fails
//! closed with `TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE` before any row is
//! written. The caller's serializable Holding transaction is required and no
//! commit is performed here.
use crate::application::offer::{
    OfferFuture, TalentHireHandoffReadRepository, TalentOfferCommandRepository,
};
use crate::domain::offer::{
    CREATE_OPERATION, CreateOfferPlan, HireHandoffSource, OfferAction, OfferApplicationFact,
    OfferChangePlan, OfferMoneyRequest, OfferMoneySnapshot, OfferReceipt, OfferRow, TalentActor,
    idempotency_conflict, money_snapshot_unavailable, offer_changed, readback_invalid,
    source_invalid_error,
};
use crate::domain::receipt::ReceiptSnapshot;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};
use std::future::Future;
use std::pin::Pin;

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'records.read') \
 AND iam_role_allows(a.role,a.company_id,'records.review')";

const OFFER_SELECT: &str = "SELECT o.id,o.company_id,o.application_id,o.application_version,\
 r.branch_id,r.department_id,o.currency,o.base_amount_minor,o.start_date,o.expires_on,o.terms,\
 o.status,o.created_by,o.approved_by,o.accepted_evidence,o.version,o.created_at,o.updated_at,\
 c.id candidate_id,c.status candidate_status,c.version candidate_version,c.retention_until \
 FROM talent_offers o \
 JOIN talent_applications a ON a.id=o.application_id \
 JOIN talent_requisitions r ON r.id=a.requisition_id \
 JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=o.company_id";

fn map_offer(row: sqlx::postgres::PgRow) -> Result<OfferRow, sqlx::Error> {
    Ok(OfferRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        application_id: row.try_get("application_id")?,
        application_version: row.try_get("application_version")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        currency: row.try_get("currency")?,
        base_amount_minor: row.try_get("base_amount_minor")?,
        start_date: row.try_get("start_date")?,
        expires_on: row.try_get("expires_on")?,
        terms: row.try_get("terms")?,
        status: row.try_get("status")?,
        created_by: row.try_get("created_by")?,
        approved_by: row.try_get("approved_by")?,
        accepted_evidence: row.try_get("accepted_evidence")?,
        version: row.try_get("version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        candidate_id: row.try_get("candidate_id")?,
        candidate_status: row.try_get("candidate_status")?,
        candidate_version: row.try_get("candidate_version")?,
        retention_until: row.try_get("retention_until")?,
    })
}

const HANDOFF_SELECT: &str = "SELECT h.id,h.company_id,h.branch_id,h.department_id,h.offer_id,\
 h.candidate_id,h.source_offer_version,h.status,h.created_at,\
 o.version offer_version,o.status offer_status,o.application_id,o.currency,o.base_amount_minor,\
 o.start_date,o.accepted_evidence,r.job_title,c.version candidate_version,c.status candidate_status,\
 c.retention_until,c.legal_name,c.email,c.phone,c.consent_evidence \
 FROM talent_hire_handoffs h \
 JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id \
 JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id \
 JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id \
 AND r.branch_id=h.branch_id AND r.department_id IS NOT DISTINCT FROM h.department_id \
 JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id";

fn map_handoff(row: sqlx::postgres::PgRow) -> Result<HireHandoffSource, sqlx::Error> {
    Ok(HireHandoffSource {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        offer_id: row.try_get("offer_id")?,
        offer_version: row.try_get("offer_version")?,
        offer_status: row.try_get("offer_status")?,
        source_offer_version: row.try_get("source_offer_version")?,
        application_id: row.try_get("application_id")?,
        candidate_id: row.try_get("candidate_id")?,
        candidate_version: row.try_get("candidate_version")?,
        candidate_status: row.try_get("candidate_status")?,
        retention_until: row.try_get("retention_until")?,
        legal_name: row.try_get("legal_name")?,
        email: row.try_get("email")?,
        phone: row.try_get("phone")?,
        consent_evidence: row.try_get("consent_evidence")?,
        accepted_evidence: row.try_get("accepted_evidence")?,
        currency: row.try_get("currency")?,
        base_amount_minor: row.try_get("base_amount_minor")?,
        start_date: row.try_get("start_date")?,
        job_title: row.try_get("job_title")?,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Finance monthly-compensation snapshot producer composed into the Talent
/// transaction. The implementation resolves the FX/rate evidence and retains the
/// snapshot row inside the caller's transaction.
pub trait PgOfferMoneySnapshotCapture: Send + Sync {
    fn capture<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor: &'a TalentActor,
        request: &'a OfferMoneyRequest,
        now: i64,
    ) -> Pin<Box<dyn Future<Output = Result<OfferMoneySnapshot, BusinessError>> + Send + 'a>>;
}

pub struct PgTalentOffers<'c> {
    connection: &'c mut PgConnection,
    capture: Option<&'c dyn PgOfferMoneySnapshotCapture>,
}

impl<'c> PgTalentOffers<'c> {
    /// Without a Finance producer port the offer command fails closed.
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self {
            connection,
            capture: None,
        }
    }

    pub fn with_money_snapshot(
        connection: &'c mut PgConnection,
        capture: &'c dyn PgOfferMoneySnapshotCapture,
    ) -> Self {
        Self {
            connection,
            capture: Some(capture),
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("TALENT_OFFER_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("TALENT_OFFER_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("talent-offer-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn current(&mut self, actor: &TalentActor, write: bool, now: i64) -> Result<bool, Error> {
        self.ready().await?;
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM assignments a WHERE {AUTHORITY} \
             AND ($5::text IS NULL OR a.role=$5::text) \
             AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$6::text) \
             OR (a.scope_kind='department' AND a.scope_node_id=$7::text)) \
             AND (NOT $8::boolean OR iam_role_allows(a.role,a.company_id,'records.write')))"
        );
        Ok(sqlx::query_scalar(AssertSqlSafe(sql))
            .bind(actor.assignment_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .bind(actor.role.as_deref())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(write)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn offer_row(
        &mut self,
        actor: &TalentActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<OfferRow>, Error> {
        self.ready().await?;
        let sql = format!(
            "{OFFER_SELECT} WHERE o.id=$1 AND o.company_id=$2 \
             AND ($3::text IS NULL OR r.branch_id=$3) \
             AND ($4::text IS NULL OR r.department_id=$4){}",
            if lock { " FOR UPDATE OF o" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_offer).transpose().map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    async fn insert_receipt(
        &mut self,
        actor: &TalentActor,
        key: &str,
        operation: &str,
        record_id: &str,
        request_hash: &str,
        result: &OfferReceipt,
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
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'talent_offer',$5,$6,$4,$7,$8)",
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
        branch_id: &str,
        department_id: Option<&str>,
        now: i64,
    ) -> Result<(), Error> {
        let authority: Value =
            serde_json::from_str(&actor.authority_json()).map_err(|_| readback_invalid())?;
        let payload = json!({
            "authority": authority,
            "id": record_id,
            "state": status,
            "companyId": actor.company_id.as_str(),
            "branchId": branch_id,
            "departmentId": department_id,
            "version": version,
        })
        .to_string();
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,'talent_offer',$3,$4,$5,$6)",
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

    async fn application_placement(
        &mut self,
        actor: &TalentActor,
        application_id: &str,
    ) -> Result<(String, Option<String>), Error> {
        let row = sqlx::query(
            "SELECT r.branch_id,r.department_id FROM talent_applications a \
             JOIN talent_requisitions r ON r.id=a.requisition_id \
             WHERE a.id=$1 AND a.company_id=$2",
        )
        .bind(application_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(source_invalid_error)?;
        Ok((row.try_get("branch_id")?, row.try_get("department_id")?))
    }

    async fn handoff_row(
        &mut self,
        actor: &TalentActor,
        id: &str,
    ) -> Result<Option<HireHandoffSource>, Error> {
        self.ready().await?;
        let sql = format!(
            "{HANDOFF_SELECT} WHERE h.id=$1 AND h.company_id=$2 \
             AND ($3::text IS NULL OR r.branch_id=$3) \
             AND ($4::text IS NULL OR r.department_id=$4)"
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(map_handoff).transpose().map_err(Into::into)
    }

    async fn readback_create(
        &mut self,
        actor: &TalentActor,
        plan: &CreateOfferPlan,
    ) -> Result<(), Error> {
        let Some(row) = self.offer_row(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.id != plan.id
            || row.company_id != plan.company_id
            || row.application_id != plan.application_id
            || row.application_version != plan.application_version
            || row.currency != plan.currency
            || row.base_amount_minor != plan.amount_minor
            || row.start_date != plan.start_date
            || row.expires_on != plan.expires_on
            || row.terms != plan.terms
            || row.status != "draft"
            || row.created_by != plan.created_by
            || row.approved_by.is_some()
            || row.accepted_evidence.is_some()
            || row.version != 1
            || row.created_at != plan.now
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_change(
        &mut self,
        actor: &TalentActor,
        plan: &OfferChangePlan,
    ) -> Result<(), Error> {
        let Some(row) = self.offer_row(actor, &plan.id, false).await? else {
            return Err(readback_invalid().into());
        };
        if row.status != plan.status
            || row.version != plan.version
            || row.approved_by != plan.approved_by
            || row.accepted_evidence != plan.accepted_evidence
            || row.updated_at != plan.now
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn handoff_exists(&mut self, id: &str) -> Result<bool, Error> {
        Ok(
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM talent_hire_handoffs WHERE id=$1)")
                .bind(id)
                .fetch_one(&mut *self.connection)
                .await?,
        )
    }
}

impl TalentOfferCommandRepository for PgTalentOffers<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> OfferFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> OfferFuture<'a, String, Self::Error> {
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
    ) -> OfferFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
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

    fn application<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> OfferFuture<'a, Option<OfferApplicationFact>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT a.id,a.company_id,a.requisition_id,r.branch_id,r.department_id,a.version,\
                 a.status,a.candidate_id,c.status candidate_status,c.version candidate_version,\
                 c.retention_until,\
                 EXISTS(SELECT 1 FROM talent_proposals p \
                        WHERE p.application_id=a.id AND p.status='accepted') has_accepted_proposal \
                 FROM talent_applications a \
                 JOIN talent_requisitions r ON r.id=a.requisition_id \
                 JOIN talent_candidates c ON c.id=a.candidate_id AND c.company_id=a.company_id \
                 WHERE a.id=$1 AND a.company_id=$2 \
                 AND ($3::text IS NULL OR r.branch_id=$3) \
                 AND ($4::text IS NULL OR r.department_id=$4)",
            )
            .bind(id)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(OfferApplicationFact {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    requisition_id: row.try_get("requisition_id")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    version: row.try_get("version")?,
                    status: row.try_get("status")?,
                    candidate_id: row.try_get("candidate_id")?,
                    candidate_status: row.try_get("candidate_status")?,
                    candidate_version: row.try_get("candidate_version")?,
                    retention_until: row.try_get("retention_until")?,
                    has_accepted_proposal: row.try_get("has_accepted_proposal")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn money_snapshot<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        offer_id: &'a str,
        input: &'a OfferMoneyRequest,
        now: i64,
    ) -> OfferFuture<'a, OfferMoneySnapshot, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let Some(capture) = self.capture else {
                return Err(money_snapshot_unavailable().into());
            };
            if input.source_id != offer_id {
                return Err(readback_invalid().into());
            }
            let connection = &mut *self.connection;
            capture
                .capture(connection, actor, input, now)
                .await
                .map_err(Into::into)
        })
    }

    fn commit_create<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a CreateOfferPlan,
        request_hash: &'a str,
    ) -> OfferFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "INSERT INTO talent_offers\
                 (id,company_id,application_id,application_version,currency,base_amount_minor,\
                 start_date,expires_on,terms,status,created_by,approved_by,accepted_evidence,\
                 version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'draft',$10,NULL,NULL,1,$11,$11) \
                 ON CONFLICT(id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(&plan.application_id)
            .bind(plan.application_version)
            .bind(&plan.currency)
            .bind(plan.amount_minor)
            .bind(&plan.start_date)
            .bind(&plan.expires_on)
            .bind(&plan.terms)
            .bind(&plan.created_by)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(idempotency_conflict().into());
            }
            let event = sqlx::query(
                "INSERT INTO talent_offer_events\
                 (id,offer_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,1,'create',$3,'Offer drafted from accepted proposal','draft',$4) \
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
            let (branch, department) = self
                .application_placement(actor, &plan.application_id)
                .await?;
            let receipt = OfferReceipt::new(plan.id.clone(), "draft", 1, None);
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
                "draft",
                1,
                &branch,
                department.as_deref(),
                plan.now,
            )
            .await?;
            self.readback_create(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }

    fn offer_for_update<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> OfferFuture<'a, Option<OfferRow>, Self::Error> {
        Box::pin(async move { self.offer_row(actor, id, true).await })
    }

    fn commit_change<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        key: &'a str,
        plan: &'a OfferChangePlan,
        request_hash: &'a str,
    ) -> OfferFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE talent_offers SET status=$5,approved_by=$6,accepted_evidence=$7,\
                 version=version+1,updated_at=$8 \
                 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$4",
            )
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .bind(&plan.expected_status)
            .bind(&plan.status)
            .bind(&plan.approved_by)
            .bind(&plan.accepted_evidence)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(offer_changed().into());
            }
            if plan.action == OfferAction::Send {
                let advanced = sqlx::query(
                    "UPDATE talent_applications SET status='offered',version=version+1,\
                     updated_at=$2 WHERE id=$1 AND status='selected'",
                )
                .bind(&plan.application_id)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if advanced != 1 {
                    return Err(offer_changed().into());
                }
            }
            if plan.action == OfferAction::AcceptOffer {
                let advanced = sqlx::query(
                    "UPDATE talent_applications SET status='accepted',version=version+1,\
                     updated_at=$2 WHERE id=$1 AND status='offered'",
                )
                .bind(&plan.application_id)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if advanced != 1 {
                    return Err(offer_changed().into());
                }
                let Some(handoff) = plan.handoff_id.as_deref() else {
                    return Err(readback_invalid().into());
                };
                let inserted = sqlx::query(
                    "INSERT INTO talent_hire_handoffs\
                     (id,offer_id,company_id,branch_id,department_id,candidate_id,\
                     source_offer_version,status,created_at) \
                     SELECT $1,o.id,o.company_id,r.branch_id,r.department_id,a.candidate_id,$2,'open',$3 \
                     FROM talent_offers o \
                     JOIN talent_applications a ON a.id=o.application_id \
                     JOIN talent_requisitions r ON r.id=a.requisition_id \
                     WHERE o.id=$4",
                )
                .bind(handoff)
                .bind(plan.version)
                .bind(plan.now)
                .bind(&plan.id)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if inserted != 1 {
                    return Err(readback_invalid().into());
                }
            }
            let event = sqlx::query(
                "INSERT INTO talent_offer_events\
                 (id,offer_id,version,action,actor_id,reason,status,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&plan.event_id)
            .bind(&plan.id)
            .bind(plan.version)
            .bind(plan.action.as_str())
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(&plan.status)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if event != 1 {
                return Err(idempotency_conflict().into());
            }
            if let Some(handoff) = plan.handoff_id.as_deref()
                && !self.handoff_exists(handoff).await?
            {
                return Err(readback_invalid().into());
            }
            let receipt = OfferReceipt::new(
                plan.id.clone(),
                &plan.status,
                plan.version,
                plan.handoff_id.clone(),
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
                &plan.branch_id,
                plan.department_id.as_deref(),
                plan.now,
            )
            .await?;
            self.readback_change(actor, plan).await?;
            Ok(receipt.to_wire())
        })
    }
}

impl TalentHireHandoffReadRepository for PgTalentOffers<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        write: bool,
        now: i64,
    ) -> OfferFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.current(actor, write, now).await })
    }

    fn handoff<'a>(
        &'a mut self,
        actor: &'a TalentActor,
        id: &'a str,
    ) -> OfferFuture<'a, Option<HireHandoffSource>, Self::Error> {
        Box::pin(async move { self.handoff_row(actor, id).await })
    }

    fn handoffs<'a>(
        &'a mut self,
        actor: &'a TalentActor,
    ) -> OfferFuture<'a, Vec<(HireHandoffSource, bool)>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = "SELECT h.id,h.company_id,h.branch_id,h.department_id,h.offer_id,h.candidate_id,\
                 h.source_offer_version,h.status,h.created_at,\
                 o.version offer_version,o.status offer_status,o.application_id,o.currency,\
                 o.base_amount_minor,o.start_date,o.accepted_evidence,r.job_title,\
                 c.version candidate_version,c.status candidate_status,c.retention_until,\
                 c.legal_name,c.email,c.phone,c.consent_evidence,\
                 (x.id IS NOT NULL) consumed \
                 FROM talent_hire_handoffs h \
                 JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id \
                 JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id \
                 JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id \
                 AND r.branch_id=h.branch_id \
                 AND r.department_id IS NOT DISTINCT FROM h.department_id \
                 JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id \
                 AND c.company_id=h.company_id \
                 LEFT JOIN talent_hire_consumption_receipts x ON x.handoff_id=h.id \
                 WHERE h.company_id=$1 \
                 AND ($2::text IS NULL OR r.branch_id=$2) \
                 AND ($3::text IS NULL OR r.department_id=$3) \
                 ORDER BY h.created_at,h.id LIMIT 200";
            let rows = sqlx::query(AssertSqlSafe(sql))
                .bind(actor.company_id.as_str())
                .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
                .bind(actor.department_id.as_ref().map(|value| value.as_str()))
                .fetch_all(&mut *self.connection)
                .await?;
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let consumed: bool = row.try_get("consumed")?;
                out.push((map_handoff(row)?, consumed));
            }
            Ok(out)
        })
    }
}
