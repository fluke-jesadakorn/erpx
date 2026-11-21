//! PostgreSQL eligibility handoff facts for the native Learning owner.
//!
//! SQL reads Learning-owned handoff/certificate/consumption rows plus the
//! current assignment and People person status already used by the retained
//! eligibility reader. The consumption command writes only the Learning receipt
//! and, for its own command entrypoint, the caller-owned command envelope. No
//! Benefits, People, IAM or Commerce row is mutated except the retained
//! `benefit_elections.learning_certificate_id` comparison read. The caller's
//! serializable Holding transaction is required.
use crate::adapters::learning::{PgLearning, map_certificate};
use crate::application::eligibility_facts::{
    BenefitCommandEnvelope, BenefitConsumptionRepository, EligibilityFuture,
    EligibilityReadRepository,
};
use crate::domain::eligibility_facts::{
    BenefitConsumptionPlan, BenefitConsumptionReceipt, EligibilityCertificateRow,
    EligibilityConsumptionRow, EligibilityScope,
};
use crate::domain::learning::{
    CertificateRow, HandoffRow, ReceiptSnapshot, idempotency_conflict, readback_invalid,
    source_invalid,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

const ELIGIBILITY_SQL: &str = r#"
SELECT c.id,c.handoff_id,c.company_id,a.branch_id,a.department_id,
 c.person_id,c.assignment_id,c.source_assignment_version,c.eligibility_code,c.valid_from,
 c.expires_on,c.completion_kind,c.evidence_ref,c.issued_by,c.created_at,
 revocation.id revocation_id,
 h.assignment_id handoff_assignment_id,h.company_id handoff_company_id,
 h.person_id handoff_person_id,h.eligibility_code handoff_eligibility_code,
 h.source_assignment_version handoff_source_assignment_version,h.status handoff_status,
 h.created_at handoff_created_at,
 a.version assignment_version,a.status assignment_status,
 p.status person_status,p.merged_into_id person_merged_into_id,
 COALESCE((SELECT jsonb_agg(jsonb_build_object(
 'id',r.id,'election_id',r.election_id,
 'source_assignment_version',r.source_assignment_version,
 'eligibility_code',r.eligibility_code,'source_digest',r.source_digest,
 'purpose',r.purpose,'actor_id',r.actor_id,'command_id',r.command_id,
 'created_at',r.created_at) ORDER BY r.created_at,r.id)
 FROM (
  SELECT * FROM learning_benefit_consumption_receipts
  WHERE certificate_id=c.id AND company_id=c.company_id
  ORDER BY created_at,id LIMIT 101
 ) r),'[]'::jsonb) consumptions
 FROM learning_certificates c
 JOIN learning_eligibility_handoffs h ON h.id=c.handoff_id AND h.company_id=c.company_id
 JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id
 JOIN hr_people p ON p.id=c.person_id AND p.company_id=c.company_id
 LEFT JOIN learning_certificate_revocations revocation
 ON revocation.certificate_id=c.id AND revocation.company_id=c.company_id
 WHERE c.company_id=$1 AND c.person_id=$2 AND c.eligibility_code=$3
 ORDER BY c.valid_from DESC,c.id LIMIT 101
"#;

const CONSUMPTION_COLUMNS: &str = "id,handoff_id,certificate_id,election_id,company_id,person_id,\
 source_assignment_version,eligibility_code,source_digest,purpose,actor_id,command_id,created_at";

fn map_consumption(row: sqlx::postgres::PgRow) -> Result<BenefitConsumptionReceipt, sqlx::Error> {
    Ok(BenefitConsumptionReceipt {
        id: row.try_get("id")?,
        handoff_id: row.try_get("handoff_id")?,
        certificate_id: row.try_get("certificate_id")?,
        election_id: row.try_get("election_id")?,
        company_id: row.try_get("company_id")?,
        person_id: row.try_get("person_id")?,
        source_assignment_version: row.try_get("source_assignment_version")?,
        eligibility_code: row.try_get("eligibility_code")?,
        source_digest: row.try_get("source_digest")?,
        purpose: row.try_get("purpose")?,
        actor_id: row.try_get("actor_id")?,
        command_id: row.try_get("command_id")?,
        created_at: row.try_get("created_at")?,
    })
}

async fn fetch_certificates(
    learning: &mut PgLearning<'_>,
    scope: &EligibilityScope,
    person_id: &str,
    eligibility_code: &str,
) -> Result<Vec<EligibilityCertificateRow>, PgTransactionError<BusinessError>> {
    learning.ready().await?;
    let rows = sqlx::query(ELIGIBILITY_SQL)
        .bind(scope.company_id())
        .bind(person_id)
        .bind(eligibility_code)
        .fetch_all(learning.connection())
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let handoff = HandoffRow {
            id: row.try_get::<String, _>("handoff_id")?,
            assignment_id: row.try_get("handoff_assignment_id")?,
            company_id: row.try_get("handoff_company_id")?,
            person_id: row.try_get("handoff_person_id")?,
            eligibility_code: row.try_get("handoff_eligibility_code")?,
            source_assignment_version: row.try_get("handoff_source_assignment_version")?,
            status: row.try_get("handoff_status")?,
            created_at: row.try_get("handoff_created_at")?,
        };
        let assignment_version = row.try_get("assignment_version")?;
        let assignment_status = row.try_get("assignment_status")?;
        let person_status = row.try_get("person_status")?;
        let person_merged_into_id = row.try_get("person_merged_into_id")?;
        let consumptions: Vec<EligibilityConsumptionRow> =
            serde_json::from_value(row.try_get("consumptions")?)
                .map_err(|_| source_invalid())?;
        out.push(EligibilityCertificateRow {
            certificate: map_certificate(row)?,
            handoff,
            assignment_version,
            assignment_status,
            person_status,
            person_merged_into_id,
            consumptions,
        });
    }
    Ok(out)
}

impl EligibilityReadRepository for PgLearning<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn eligibility_certificates<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        person_id: &'a str,
        eligibility_code: &'a str,
    ) -> EligibilityFuture<'a, Vec<EligibilityCertificateRow>, Self::Error> {
        Box::pin(async move { fetch_certificates(self, scope, person_id, eligibility_code).await })
    }
}

impl BenefitConsumptionRepository for PgLearning<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn benefit_certificate_rows<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        person_id: &'a str,
        eligibility_code: &'a str,
    ) -> EligibilityFuture<'a, Vec<EligibilityCertificateRow>, Self::Error> {
        Box::pin(async move { fetch_certificates(self, scope, person_id, eligibility_code).await })
    }

    fn lock_benefit_certificate<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        certificate_id: &'a str,
    ) -> EligibilityFuture<'a, Option<CertificateRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT c.id,c.handoff_id,c.company_id,a.branch_id,a.department_id,c.person_id,\
                 c.assignment_id,c.source_assignment_version,c.eligibility_code,c.valid_from,\
                 c.expires_on,c.completion_kind,c.evidence_ref,c.issued_by,c.created_at,\
                 revocation.id revocation_id \
                 FROM learning_certificates c \
                 JOIN learning_assignments a ON a.id=c.assignment_id AND a.company_id=c.company_id \
                 LEFT JOIN learning_certificate_revocations revocation \
                 ON revocation.certificate_id=c.id AND revocation.company_id=c.company_id \
                 WHERE c.id=$1 AND c.company_id=$2 FOR SHARE OF c, a",
            )
            .bind(certificate_id)
            .bind(scope.company_id())
            .fetch_optional(self.connection())
            .await?;
            row.map(map_certificate).transpose().map_err(Into::into)
        })
    }

    fn benefit_consumption<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        election_id: &'a str,
    ) -> EligibilityFuture<'a, Option<BenefitConsumptionReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!(
                "SELECT {CONSUMPTION_COLUMNS} FROM learning_benefit_consumption_receipts \
                 WHERE election_id=$1 AND company_id=$2"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(election_id)
                .bind(scope.company_id())
                .fetch_optional(self.connection())
                .await?;
            row.map(map_consumption).transpose().map_err(Into::into)
        })
    }

    fn benefit_consumption_by_command<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        command_id: &'a str,
    ) -> EligibilityFuture<'a, Option<BenefitConsumptionReceipt>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let sql = format!(
                "SELECT {CONSUMPTION_COLUMNS} FROM learning_benefit_consumption_receipts \
                 WHERE command_id=$1 AND company_id=$2"
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(command_id)
                .bind(scope.company_id())
                .fetch_optional(self.connection())
                .await?;
            row.map(map_consumption).transpose().map_err(Into::into)
        })
    }

    fn retained_election_certificate<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        election_id: &'a str,
    ) -> EligibilityFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT learning_certificate_id FROM benefit_elections \
                 WHERE id=$1 AND company_id=$2",
            )
            .bind(election_id)
            .bind(scope.company_id())
            .fetch_optional(self.connection())
            .await?;
            match row {
                Some(row) => row
                    .try_get::<Option<String>, _>("learning_certificate_id")
                    .map_err(Into::into),
                None => Ok(None),
            }
        })
    }

    fn digest<'a>(
        &'a mut self,
        value: &'a str,
    ) -> EligibilityFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(self.connection())
                    .await?,
            )
        })
    }

    fn insert_benefit_consumption<'a>(
        &'a mut self,
        plan: &'a BenefitConsumptionPlan,
        envelope: Option<&'a BenefitCommandEnvelope>,
    ) -> EligibilityFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("learning-benefit:{}", plan.election_id))
                .execute(self.connection())
                .await?;
            let inserted = sqlx::query(
                "INSERT INTO learning_benefit_consumption_receipts\
                 (id,handoff_id,certificate_id,election_id,company_id,person_id,\
                 source_assignment_version,eligibility_code,source_digest,purpose,actor_id,\
                 command_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
                 ON CONFLICT(election_id) DO NOTHING",
            )
            .bind(&plan.id)
            .bind(&plan.handoff_id)
            .bind(&plan.certificate_id)
            .bind(&plan.election_id)
            .bind(&plan.company_id)
            .bind(&plan.person_id)
            .bind(plan.source_assignment_version)
            .bind(&plan.eligibility_code)
            .bind(&plan.source_digest)
            .bind(&plan.purpose)
            .bind(&plan.actor_id)
            .bind(&plan.command_id)
            .bind(plan.created_at)
            .execute(self.connection())
            .await?
            .rows_affected()
                == 1;
            if let (true, Some(envelope)) = (inserted, envelope) {
                let changed = sqlx::query(
                    "INSERT INTO command_receipts\
                     (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
                     authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                     ON CONFLICT(command_id) DO NOTHING",
                )
                .bind(&plan.command_id)
                .bind(envelope.operation)
                .bind(&envelope.actor_id)
                .bind(&envelope.request_hash)
                .bind(&plan.id)
                .bind(
                    serde_json::json!({"id": plan.id, "status": "consumed", "version": 1})
                        .to_string(),
                )
                .bind(envelope.now)
                .bind(&envelope.authority_json)
                .execute(self.connection())
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(idempotency_conflict().into());
                }
                sqlx::query(
                    "INSERT INTO audit_events\
                     (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                     record_version,purpose,created_at,authority_json) \
                     VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'learning_benefit_consumption',\
                     $5,1,$4,$6,$7)",
                )
                .bind(&plan.command_id)
                .bind(&envelope.actor_id)
                .bind(&envelope.assignment_id)
                .bind(envelope.operation)
                .bind(&plan.id)
                .bind(envelope.now)
                .bind(&envelope.authority_json)
                .execute(self.connection())
                .await?;
                let payload = serde_json::json!({
                    "authority": serde_json::from_str::<serde_json::Value>(&envelope.authority_json)
                        .map_err(|_| readback_invalid())?,
                    "id": plan.id,
                    "companyId": plan.company_id,
                    "personId": plan.person_id,
                    "electionId": plan.election_id,
                    "certificateId": plan.certificate_id,
                    "handoffId": plan.handoff_id,
                    "eligibilityCode": plan.eligibility_code,
                    "sourceAssignmentVersion": plan.source_assignment_version,
                    "version": 1,
                })
                .to_string();
                sqlx::query(
                    "INSERT INTO outbox_events\
                     (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
                     payload_json,created_at) \
                     VALUES(gen_random_uuid()::text,$1,$2,'learning_benefit_consumption',$3,1,$4,$5)",
                )
                .bind(&plan.command_id)
                .bind(envelope.operation)
                .bind(&plan.id)
                .bind(payload)
                .bind(envelope.now)
                .execute(self.connection())
                .await?;
            }
            Ok(inserted)
        })
    }

    fn command_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> EligibilityFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(self.connection())
            .await?;
            row.map(|row| {
                Ok::<_, Self::Error>(ReceiptSnapshot {
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
}

/// Acquire the Learning eligibility source bundle lock before a Benefits
/// election/transition reads or consumes a certificate. The enclosing owner
/// unit supplies the serializable transaction and its earlier authority/source
/// locks; this helper performs no transaction check of its own.
pub async fn lock_learning_benefit_sources(
    connection: &mut sqlx::PgConnection,
) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query(
        "LOCK TABLE learning_assignments,learning_eligibility_handoffs,\
         learning_certificates,learning_certificate_revocations,\
         learning_benefit_consumption_receipts IN SHARE MODE",
    )
    .execute(connection)
    .await?;
    Ok(())
}
