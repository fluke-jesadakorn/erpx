//! PostgreSQL eligibility handoff facts for the native Learning owner.
//!
//! SQL reads Learning-owned handoff/certificate/consumption rows plus the
//! current assignment and People person status already used by the retained
//! eligibility reader. No Benefits, People, IAM or Commerce row is mutated and
//! the caller's serializable Holding transaction is required.
use crate::adapters::learning::{PgLearning, map_certificate};
use crate::application::eligibility_facts::{EligibilityFuture, EligibilityReadRepository};
use crate::domain::eligibility_facts::{
    EligibilityCertificateRow, EligibilityConsumptionRow, EligibilityScope,
};
use crate::domain::learning::{HandoffRow, source_invalid};
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

impl EligibilityReadRepository for PgLearning<'_> {
    type Error = PgTransactionError<BusinessError>;

    fn eligibility_certificates<'a>(
        &'a mut self,
        scope: &'a EligibilityScope,
        person_id: &'a str,
        eligibility_code: &'a str,
    ) -> EligibilityFuture<'a, Vec<EligibilityCertificateRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(ELIGIBILITY_SQL)
                .bind(scope.company_id())
                .bind(person_id)
                .bind(eligibility_code)
                .fetch_all(self.connection())
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
        })
    }
}
