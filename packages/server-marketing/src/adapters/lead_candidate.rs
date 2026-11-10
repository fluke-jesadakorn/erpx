//! PostgreSQL adapter for native Marketing campaign, candidate, engagement and
//! handoff commands and owner reads.
//!
//! Only Marketing-owned relations are read or written. Current IAM is re-read
//! through the existing assignments/functions. CRM handoff is injected through
//! the public port; no CRM row is written here. The caller owns the serializable
//! transaction and this adapter never commits.
use crate::adapters::lead_candidate_use_cases::{
    MarketingAuditEntry, MarketingLeadRepository, MarketingOutboxEntry, MarketingReceiptEntry,
    MarketingStoredReceipt,
};
use crate::domain::lead_candidate::{
    ASSET_LIMIT, CANDIDATE_LIMIT, HISTORY_LIMIT, MarketingAssetProfile,
    MarketingCampaignCreateInput, MarketingCampaignReadback, MarketingCandidateReadback,
    MarketingEngagementRow, MarketingFormAssetFacts, MarketingLeadActor,
    MarketingLeadCandidateInput, MarketingLeadCandidateRow,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments a \
 WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
 AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
 AND iam_role_allows(a.role,a.company_id,'records.write') \
 AND ($5::text IS NULL OR a.role=$5::text) \
 AND (a.scope_kind='holding' \
   OR (a.scope_kind='branch' AND a.scope_node_id=$6::text) \
   OR (a.scope_kind='department' AND a.scope_node_id=$7::text)) \
 AND (jsonb_array_length(a.modules_json::jsonb)=0 \
   OR a.modules_json::jsonb @> '[\"marketing\"]'::jsonb))";

#[derive(Clone, Copy, Debug, Default)]
pub struct PgMarketingLead;

impl PgMarketingLead {
    async fn ready(c: &mut PgConnection) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("MARKETING_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("MARKETING_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    fn scope(actor: &MarketingLeadActor) -> (Option<&str>, Option<&str>) {
        (
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
        )
    }

    async fn candidate_row(
        c: &mut PgConnection,
        actor: &MarketingLeadActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<MarketingLeadCandidateRow>, Error> {
        let (branch, department) = Self::scope(actor);
        let sql = if lock {
            "SELECT c.*,f.name form_name FROM marketing_lead_candidates c \
             JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id \
             WHERE c.id=$1 AND c.company_id=$2 \
             AND ($3::text IS NULL OR c.branch_id=$3) \
             AND ($4::text IS NULL OR c.department_id=$4) FOR UPDATE OF c"
        } else {
            "SELECT c.*,f.name form_name FROM marketing_lead_candidates c \
             JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id \
             WHERE c.id=$1 AND c.company_id=$2 \
             AND ($3::text IS NULL OR c.branch_id=$3) \
             AND ($4::text IS NULL OR c.department_id=$4)"
        };
        let row = sqlx::query(sql)
            .bind(id)
            .bind(&actor.company_id)
            .bind(branch)
            .bind(department)
            .fetch_optional(&mut *c)
            .await?;
        row.map(|row| {
            Ok::<_, Error>(MarketingLeadCandidateRow {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                form_id: row.try_get("form_id")?,
                form_version: row.try_get("form_version")?,
                form_name: row.try_get("form_name")?,
                contact_name: row.try_get("contact_name")?,
                contact_detail: row.try_get("contact_detail")?,
                consent_evidence: row.try_get("consent_evidence")?,
                attribution_json: row.try_get("attribution_json")?,
                brief: row.try_get("brief")?,
                status: row.try_get("status")?,
                crm_lead_id: row.try_get("crm_lead_id")?,
                created_by: row.try_get("created_by")?,
                reviewed_by: row.try_get("reviewed_by")?,
                version: row.try_get("version")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .transpose()
    }
}

impl MarketingLeadRepository for PgMarketingLead {
    fn ready<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            Ok(())
        })
    }

    fn lock_command<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("marketing-command:{key}"))
                .execute(&mut *c)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        value: &'a str,
    ) -> PgWorkFuture<'a, String, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            Ok(
                sqlx::query_scalar("SELECT encode(sha256(convert_to($1,'UTF8')),'hex')")
                    .bind(value)
                    .fetch_one(&mut *c)
                    .await?,
            )
        })
    }

    fn receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingStoredReceipt>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,record_id FROM command_receipts \
                 WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *c)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(MarketingStoredReceipt {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    record_id: row.try_get("record_id")?,
                })
            })
            .transpose()
        })
    }

    fn current_authority<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let (branch, department) = Self::scope(actor);
            Ok(sqlx::query_scalar(AUTHORITY)
                .bind(&actor.assignment_id)
                .bind(&actor.user_id)
                .bind(&actor.company_id)
                .bind(now.get())
                .bind(actor.role.as_deref())
                .bind(branch)
                .bind(department)
                .fetch_one(&mut *c)
                .await?)
        })
    }

    fn form_asset<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        form_id: &'a str,
        form_version: i64,
    ) -> PgWorkFuture<'a, Option<MarketingFormAssetFacts>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let row: Option<String> = sqlx::query_scalar(
                "SELECT configuration_json FROM marketing_assets \
                 WHERE id=$1 AND company_id=$2 AND kind='form' AND status='active' AND version=$3",
            )
            .bind(form_id)
            .bind(company)
            .bind(form_version)
            .fetch_optional(&mut *c)
            .await?;
            Ok(row.map(|configuration_json| MarketingFormAssetFacts { configuration_json }))
        })
    }

    fn asset_create<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        actor: &'a MarketingLeadActor,
        input: &'a crate::domain::lead_candidate::MarketingAssetCreateInput,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let changed = sqlx::query(
                "INSERT INTO marketing_assets\
                 (id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,'draft',$7,NULL,1,$8,$8) ON CONFLICT(id) DO NOTHING",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(&input.kind)
            .bind(input.name())
            .bind(&input.configuration_json)
            .bind(&input.preview_json)
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn asset_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        id: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingAssetProfile>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let row = sqlx::query(
                "SELECT id,company_id,kind,name,status,version,configuration_json,preview_json \
                 FROM marketing_assets WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(company)
            .fetch_optional(&mut *c)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(MarketingAssetProfile {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    kind: row.try_get("kind")?,
                    name: row.try_get("name")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    configuration_json: row.try_get("configuration_json")?,
                    preview_json: row.try_get("preview_json")?,
                })
            })
            .transpose()
        })
    }

    fn campaign_create<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        actor: &'a MarketingLeadActor,
        input: &'a MarketingCampaignCreateInput,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let changed = sqlx::query(
                "INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(company_id,code) DO NOTHING",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(input.code())
            .bind(input.name())
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn campaign_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        id: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingCampaignReadback>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let row = sqlx::query(
                "SELECT id,code,name,version FROM marketing_campaigns WHERE id=$1 AND company_id=$2",
            )
            .bind(id)
            .bind(company)
            .fetch_optional(&mut *c)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(MarketingCampaignReadback {
                    id: row.try_get("id")?,
                    code: row.try_get("code")?,
                    name: row.try_get("name")?,
                    version: row.try_get("version")?,
                })
            })
            .transpose()
        })
    }

    fn candidate_create<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        actor: &'a MarketingLeadActor,
        input: &'a MarketingLeadCandidateInput,
        attribution_json: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let (branch, department) = Self::scope(actor);
            let branch = branch.ok_or_else(crate::domain::lead_candidate::branch_scope_required)?;
            let changed = sqlx::query(
                "INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,\
                 form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,\
                 brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'pending',NULL,$12,NULL,1,$13,$13)",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(branch)
            .bind(department)
            .bind(&input.form_id)
            .bind(input.form_version)
            .bind(crate::domain::lead_candidate::js_trim(&input.contact_name))
            .bind(crate::domain::lead_candidate::js_trim(&input.contact_detail))
            .bind(crate::domain::lead_candidate::js_trim(&input.consent_evidence))
            .bind(attribution_json)
            .bind(crate::domain::lead_candidate::js_trim(&input.brief))
            .bind(&actor.user_id)
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn candidate<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        lock: bool,
    ) -> PgWorkFuture<'a, Option<MarketingLeadCandidateRow>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            PgMarketingLead::candidate_row(c, actor, id, lock).await
        })
    }

    fn candidate_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
    ) -> PgWorkFuture<'a, Option<MarketingCandidateReadback>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let row = PgMarketingLead::candidate_row(c, actor, id, false).await?;
            Ok(row.map(|row| MarketingCandidateReadback {
                id: row.id,
                status: row.status,
                version: row.version,
                crm_lead_id: row.crm_lead_id,
                created_by: row.created_by,
                form_id: row.form_id,
                form_version: row.form_version,
                branch_id: row.branch_id,
                department_id: row.department_id,
                contact_name: row.contact_name,
                contact_detail: row.contact_detail,
                consent_evidence: row.consent_evidence,
                attribution_json: row.attribution_json,
                brief: row.brief,
            }))
        })
    }

    fn asset_profiles<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        limit: usize,
    ) -> PgWorkFuture<'a, Vec<MarketingAssetProfile>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let rows = sqlx::query(
                "SELECT id,company_id,kind,name,status,version,configuration_json,preview_json \
                 FROM marketing_assets WHERE company_id=$1 \
                 ORDER BY updated_at DESC,id LIMIT $2",
            )
            .bind(company)
            .bind(limit.min(ASSET_LIMIT + 1) as i64)
            .fetch_all(&mut *c)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(MarketingAssetProfile {
                        id: row.try_get("id")?,
                        company_id: row.try_get("company_id")?,
                        kind: row.try_get("kind")?,
                        name: row.try_get("name")?,
                        status: row.try_get("status")?,
                        version: row.try_get("version")?,
                        configuration_json: row.try_get("configuration_json")?,
                        preview_json: row.try_get("preview_json")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn candidates<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        limit: usize,
    ) -> PgWorkFuture<'a, Vec<MarketingLeadCandidateRow>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let (branch, department) = Self::scope(actor);
            let rows = sqlx::query(
                "SELECT c.*,f.name form_name FROM marketing_lead_candidates c \
                 JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id \
                 WHERE c.company_id=$1 AND ($2::text IS NULL OR c.branch_id=$2) \
                 AND ($3::text IS NULL OR c.department_id=$3) \
                 ORDER BY c.updated_at DESC,c.id LIMIT $4",
            )
            .bind(&actor.company_id)
            .bind(branch)
            .bind(department)
            .bind(limit.min(CANDIDATE_LIMIT + 1) as i64)
            .fetch_all(&mut *c)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(MarketingLeadCandidateRow {
                        id: row.try_get("id")?,
                        company_id: row.try_get("company_id")?,
                        branch_id: row.try_get("branch_id")?,
                        department_id: row.try_get("department_id")?,
                        form_id: row.try_get("form_id")?,
                        form_version: row.try_get("form_version")?,
                        form_name: row.try_get("form_name")?,
                        contact_name: row.try_get("contact_name")?,
                        contact_detail: row.try_get("contact_detail")?,
                        consent_evidence: row.try_get("consent_evidence")?,
                        attribution_json: row.try_get("attribution_json")?,
                        brief: row.try_get("brief")?,
                        status: row.try_get("status")?,
                        crm_lead_id: row.try_get("crm_lead_id")?,
                        created_by: row.try_get("created_by")?,
                        reviewed_by: row.try_get("reviewed_by")?,
                        version: row.try_get("version")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn history<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        candidate_id: &'a str,
        limit: usize,
    ) -> PgWorkFuture<'a, Vec<MarketingEngagementRow>, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let rows = sqlx::query(
                "SELECT id,candidate_id,version,event_type,channel,evidence,actor_id,created_at \
                 FROM marketing_engagement_events WHERE candidate_id=$1 \
                 ORDER BY version,id LIMIT $2",
            )
            .bind(candidate_id)
            .bind(limit.min(HISTORY_LIMIT + 1) as i64)
            .fetch_all(&mut *c)
            .await?;
            rows.iter()
                .map(|row| {
                    Ok(MarketingEngagementRow {
                        id: row.try_get("id")?,
                        candidate_id: row.try_get("candidate_id")?,
                        version: row.try_get("version")?,
                        event_type: row.try_get("event_type")?,
                        channel: row.try_get("channel")?,
                        evidence: row.try_get("evidence")?,
                        actor_id: row.try_get("actor_id")?,
                        created_at: row.try_get("created_at")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn update_candidate_engagement<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        expected_version: i64,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let (branch, department) = Self::scope(actor);
            let changed = sqlx::query(
                "UPDATE marketing_lead_candidates SET version=version+1,updated_at=$4 \
                 WHERE id=$1 AND company_id=$2 AND status='pending' AND version=$3 \
                 AND ($5::text IS NULL OR branch_id=$5) \
                 AND ($6::text IS NULL OR department_id=$6)",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(expected_version)
            .bind(now)
            .bind(branch)
            .bind(department)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn update_candidate_handoff<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        id: &'a str,
        expected_version: i64,
        crm_lead_id: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let (branch, department) = Self::scope(actor);
            let changed = sqlx::query(
                "UPDATE marketing_lead_candidates SET status='handed_off',crm_lead_id=$4,\
                 reviewed_by=$5,version=version+1,updated_at=$6 \
                 WHERE id=$1 AND company_id=$2 AND status='pending' AND version=$3 \
                 AND ($7::text IS NULL OR branch_id=$7) \
                 AND ($8::text IS NULL OR department_id=$8)",
            )
            .bind(id)
            .bind(&actor.company_id)
            .bind(expected_version)
            .bind(crm_lead_id)
            .bind(&actor.user_id)
            .bind(now)
            .bind(branch)
            .bind(department)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_engagement<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        id: &'a str,
        candidate_id: &'a str,
        version: i64,
        event_type: &'a str,
        channel: &'a str,
        evidence: &'a str,
        actor_id: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let changed = sqlx::query(
                "INSERT INTO marketing_engagement_events\
                 (id,candidate_id,version,event_type,channel,evidence,actor_id,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(candidate_id)
            .bind(version)
            .bind(event_type)
            .bind(channel)
            .bind(evidence)
            .bind(actor_id)
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a MarketingReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let authority = entry.actor.authority_json();
            let changed = sqlx::query(
                "INSERT INTO command_receipts\
                 (command_id,operation,actor_id,request_hash,record_id,result_json,created_at,\
                 authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) \
                 ON CONFLICT(command_id) DO NOTHING",
            )
            .bind(entry.key)
            .bind(entry.operation)
            .bind(&entry.actor.user_id)
            .bind(entry.request_hash)
            .bind(entry.record_id)
            .bind(entry.result_json)
            .bind(now.get())
            .bind(authority)
            .execute(&mut *c)
            .await?
            .rows_affected();
            Ok(changed == 1)
        })
    }

    fn audit<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a MarketingLeadActor,
        entry: &'a MarketingAuditEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            sqlx::query(
                "INSERT INTO audit_events\
                 (id,command_id,actor_id,assignment_id,action,record_type,record_id,\
                 record_version,purpose,created_at,authority_json) \
                 VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$4,$8,$9)",
            )
            .bind(entry.key)
            .bind(&actor.user_id)
            .bind(&actor.assignment_id)
            .bind(entry.operation)
            .bind(entry.record_type)
            .bind(entry.record_id)
            .bind(entry.version)
            .bind(now.get())
            .bind(entry.authority_json)
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }

    fn outbox<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a MarketingOutboxEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError> {
        Box::pin(async move {
            PgMarketingLead::ready(c).await?;
            let authority: Value =
                serde_json::from_str(entry.authority_json).unwrap_or(Value::Null);
            let company = authority.get("companyId").cloned().unwrap_or(Value::Null);
            let branch = authority.get("branchId").cloned().unwrap_or(Value::Null);
            let department = authority
                .get("departmentId")
                .cloned()
                .unwrap_or(Value::Null);
            let payload = json!({
                "authority": authority,
                "id": entry.record_id,
                "state": entry.state,
                "companyId": company,
                "branchId": branch,
                "departmentId": department,
                "version": entry.version,
            })
            .to_string();
            sqlx::query(
                "INSERT INTO outbox_events\
                 (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
                 payload_json,created_at) VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(entry.key)
            .bind(entry.operation)
            .bind(entry.record_type)
            .bind(entry.record_id)
            .bind(entry.version)
            .bind(payload)
            .bind(now.get())
            .execute(&mut *c)
            .await?;
            Ok(())
        })
    }
}
