//! PostgreSQL adapter for the native Identity Access role/review slice.
//!
//! SQL is Identity-owned (`iam_roles`, `iam_role_revisions`, `iam_reviews`,
//! `iam_access_review_campaigns`, `iam_access_review_items`,
//! `iam_access_review_events`, `assignments`, `active_work_contexts`,
//! `iam_members`, `iam_audit`) plus the shared command receipt, audit and outbox
//! tables. No Organization, People, Finance or other owner row is mutated. The
//! caller's serializable Holding transaction is required and no commit is
//! performed here.
use crate::application::iam_roles::{IamRolesFuture, IamRolesReviewRepository};
use crate::domain::iam_roles::{
    ACCESS_REVIEW_CANCEL_OPERATION, ACCESS_REVIEW_DECIDE_OPERATION, ACCESS_REVIEW_START_OPERATION,
    AccessReviewCancelPlan, AccessReviewDecisionPlan, AccessReviewStartPlan, CampaignRow,
    IamReceiptSnapshot, IamRoleActor, ProposerAuthority, REVIEW_DECIDE_OPERATION,
    REVIEW_KIND_ROLE_PUBLISH, ROLE_SAVE_OPERATION, ReviewAssignment, ReviewDecisionPlan,
    ReviewItemRow, ReviewRow, RevocationGuardFacts, RoleRow, RoleSavePlan,
    access_review_campaign_changed, access_review_item_changed, idempotency_conflict,
    readback_invalid, review_changed, review_source_changed, role_changed,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn source_invalid() -> BusinessError {
    crate::domain::iam_roles::source_invalid()
}

fn parse_strings(value: String) -> Result<Vec<String>, sqlx::Error> {
    serde_json::from_str(&value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

pub struct PgIamRolesReviews<'c> {
    connection: &'c mut PgConnection,
    /// Platform commands keep their roles/reviews in the Control database and
    /// therefore write Control receipts/audit without a Holding outbox.
    platform: bool,
}

impl<'c> PgIamRolesReviews<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self {
            connection,
            platform: false,
        }
    }

    /// The caller must pass a serializable Control transaction and a Platform
    /// actor. Holding callers keep [`Self::new`].
    pub fn new_platform(connection: &'c mut PgConnection) -> Self {
        Self {
            connection,
            platform: true,
        }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("IAM_ROLES_REVIEWS_TRANSACTION_REQUIRED").into());
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(PersistenceError::new("IAM_ROLES_REVIEWS_SERIALIZABLE_REQUIRED").into());
        }
        Ok(())
    }

    async fn lock_command(&mut self, key: &str) -> Result<(), Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("iam-roles-review-command:{key}"))
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }

    async fn holding_authority(
        &mut self,
        actor: &IamRoleActor,
        permission: &str,
        now: i64,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments a \
             JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id \
             LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id \
             LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id \
             JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' \
             JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
              AND v.revision=r.published_revision \
             WHERE a.id=$1 AND a.user_id=$2 AND a.company_id=$3 AND a.revoked_at IS NULL \
             AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,a.company_id,$4) \
             AND ($5::text IS NULL OR a.role=$5::text) \
             AND ($6::bigint IS NULL OR a.version=$6::bigint) \
             AND ($7::integer IS NULL OR v.revision=$7::integer) \
             AND iam_role_allows(a.role,a.company_id,$8) AND m.status='active' \
             AND ($9::integer IS NULL OR m.version=$9::integer) \
             AND (m.login_policy IS DISTINCT FROM 'directory' OR (i.enabled=true \
              AND c.status='active' AND COALESCE(i.observed_at,0)>$4-300000)) \
             AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions x WHERE x.session_id=$10 \
              AND x.company_id=$3 AND x.user_id=$2) \
             AND (m.login_policy IS DISTINCT FROM 'emergency' OR ($11::bigint IS NOT NULL \
              AND $11::bigint>$4::bigint)) \
             AND EXISTS(SELECT 1 FROM active_work_contexts w WHERE w.session_id=$10 \
              AND w.company_id=$3 AND w.assignment_id=a.id AND w.assignment_version=a.version \
              AND ($12::bigint IS NULL OR w.revision=$12::bigint)))",
        )
        .bind(actor.assignment_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.company_id.as_str())
        .bind(now)
        .bind(actor.role.as_deref())
        .bind(actor.assignment_version)
        .bind(actor.role_revision)
        .bind(permission)
        .bind(actor.identity_version)
        .bind(actor.session_id.as_str())
        .bind(actor.emergency_until)
        .bind(actor.authority_revision)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    async fn platform_authority(
        &mut self,
        actor: &IamRoleActor,
        permission: &str,
        now: i64,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM platform_assignments a \
             JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',$3) AND r.status='active' \
             JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
              AND v.revision=r.published_revision \
             WHERE a.id=$1 AND a.user_id=$2 AND a.revoked_at IS NULL \
             AND iam_assignment_active(a.id,$4) \
             AND ($5::bigint IS NULL OR a.version=$5::bigint) \
             AND iam_role_allows(a.role,$3,$6) \
             AND iam_principal_active(a.user_id,$3,$4))",
        )
        .bind(actor.assignment_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.company_id.as_str())
        .bind(now)
        .bind(actor.assignment_version)
        .bind(permission)
        .fetch_one(&mut *self.connection)
        .await?)
    }

    #[allow(clippy::too_many_arguments)]
    async fn effects_with_hash(
        &mut self,
        actor: &IamRoleActor,
        key: &str,
        request_hash: &str,
        operation: &str,
        record_type: &str,
        record_id: &str,
        record_version: i64,
        result_json: &str,
        evidence: &Value,
        outbox: &Value,
        now: i64,
    ) -> Result<(), Error> {
        let authority = actor.authority_json();
        if self.platform {
            let envelope = json!({
                "result": serde_json::from_str::<Value>(result_json)
                    .map_err(|_| readback_invalid())?,
                "operation": operation,
                "recordId": record_id,
                "authority": serde_json::from_str::<Value>(&authority)
                    .map_err(|_| readback_invalid())?,
            });
            let receipt = sqlx::query(
                "INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) \
                 VALUES($1,$2,$3,$4,$5::jsonb,$6) ON CONFLICT(id) DO NOTHING",
            )
            .bind(key)
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(request_hash)
            .bind(envelope.to_string())
            .bind(now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if receipt != 1 {
                return Err(idempotency_conflict().into());
            }
            return self
                .audit(
                    actor,
                    key,
                    operation,
                    record_type,
                    record_id,
                    record_version,
                    evidence,
                    now,
                )
                .await;
        }
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
        .bind(result_json)
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if receipt != 1 {
            return Err(idempotency_conflict().into());
        }
        self.audit(
            actor,
            key,
            operation,
            record_type,
            record_id,
            record_version,
            evidence,
            now,
        )
        .await?;
        self.outbox(
            actor,
            key,
            operation,
            record_type,
            record_id,
            record_version,
            outbox,
            now,
        )
        .await
    }

    async fn audit(
        &mut self,
        actor: &IamRoleActor,
        key: &str,
        operation: &str,
        record_type: &str,
        record_id: &str,
        record_version: i64,
        evidence: &Value,
        now: i64,
    ) -> Result<(), Error> {
        let mut full = evidence.clone();
        if let Some(object) = full.as_object_mut() {
            object.insert(
                "authority".into(),
                serde_json::from_str::<Value>(&actor.authority_json())
                    .map_err(|_| readback_invalid())?,
            );
        }
        sqlx::query(
            "INSERT INTO iam_audit\
             (id,company_id,actor_id,assignment_id,action,target_id,evidence,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6::jsonb,$7)",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.user_id.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(operation)
        .bind(record_id)
        .bind(full.to_string())
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        if self.platform {
            return Ok(());
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
        .bind(record_type)
        .bind(record_id)
        .bind(record_version)
        .bind(now)
        .bind(actor.authority_json())
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn outbox(
        &mut self,
        actor: &IamRoleActor,
        key: &str,
        operation: &str,
        record_type: &str,
        record_id: &str,
        record_version: i64,
        payload: &Value,
        now: i64,
    ) -> Result<(), Error> {
        let mut payload = payload.clone();
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "authority".into(),
                serde_json::from_str::<Value>(&actor.authority_json())
                    .map_err(|_| readback_invalid())?,
            );
        }
        sqlx::query(
            "INSERT INTO outbox_events\
             (id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,\
             payload_json,created_at) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(key)
        .bind(operation)
        .bind(record_type)
        .bind(record_id)
        .bind(record_version)
        .bind(payload.to_string())
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn role_row(
        &mut self,
        actor: &IamRoleActor,
        id: &str,
        lock: bool,
    ) -> Result<Option<RoleRow>, Error> {
        self.ready().await?;
        let sql = format!(
            "SELECT company_id,id,name,built_in,published_revision::bigint published_revision,status \
             FROM iam_roles WHERE id=$1 AND company_id IN ($2,'*'){}",
            if lock { " FOR UPDATE" } else { "" }
        );
        let row = sqlx::query(AssertSqlSafe(sql))
            .bind(id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok::<RoleRow, sqlx::Error>(RoleRow {
                company_id: row.try_get("company_id")?,
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                built_in: row.try_get("built_in")?,
                published_revision: row.try_get("published_revision")?,
                status: row.try_get("status")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn review_row(
        &mut self,
        actor: &IamRoleActor,
        id: &str,
    ) -> Result<Option<ReviewRow>, Error> {
        self.ready().await?;
        let row = sqlx::query(
            "SELECT id,kind,payload,created_by,created_person,status,version::bigint version,reason \
             FROM iam_reviews WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok::<ReviewRow, sqlx::Error>(ReviewRow {
                id: row.try_get("id")?,
                kind: row.try_get("kind")?,
                payload: row.try_get("payload")?,
                created_by: row.try_get("created_by")?,
                created_person: row.try_get("created_person")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                reason: row.try_get("reason")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn assignment_row(
        &mut self,
        actor: &IamRoleActor,
        id: &str,
    ) -> Result<Option<ReviewAssignment>, Error> {
        self.ready().await?;
        let row = sqlx::query(
            "SELECT id,user_id,role,scope_kind,scope_node_id,source,valid_until,version,revoked_at \
             FROM assignments WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok::<ReviewAssignment, sqlx::Error>(ReviewAssignment {
                id: row.try_get("id")?,
                user_id: row.try_get("user_id")?,
                role: row.try_get("role")?,
                scope_kind: row.try_get("scope_kind")?,
                scope_node_id: row.try_get("scope_node_id")?,
                source: row.try_get("source")?,
                valid_until: row.try_get("valid_until")?,
                version: row.try_get("version")?,
                revoked_at: row.try_get("revoked_at")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn campaign_row(
        &mut self,
        actor: &IamRoleActor,
        id: &str,
    ) -> Result<Option<CampaignRow>, Error> {
        self.ready().await?;
        let row = sqlx::query(
            "SELECT id,status,version::bigint version,created_by FROM iam_access_review_campaigns \
             WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok::<CampaignRow, sqlx::Error>(CampaignRow {
                id: row.try_get("id")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                created_by: row.try_get("created_by")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn readback_role(
        &mut self,
        actor: &IamRoleActor,
        plan: &RoleSavePlan,
    ) -> Result<(), Error> {
        let revision = sqlx::query(
            "SELECT v.permissions_json::text permissions_json,v.grantable_json::text grantable_json,\
             v.scopes_json::text scopes_json,r.published_revision::bigint published_revision \
             FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id \
              AND v.role_id=r.id AND v.revision=$3 \
             WHERE r.company_id=$1 AND r.id=$2",
        )
        .bind(actor.company_id.as_str())
        .bind(&plan.id)
        .bind(plan.revision)
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        let permissions = parse_strings(revision.try_get("permissions_json")?)?;
        let grantable = parse_strings(revision.try_get("grantable_json")?)?;
        let scopes = parse_strings(revision.try_get("scopes_json")?)?;
        if permissions != plan.permissions
            || grantable != plan.grantable_permissions
            || scopes != plan.scopes
            || revision.try_get::<i64, _>("published_revision")? != plan.expected_version
        {
            return Err(readback_invalid().into());
        }
        let review = sqlx::query(
            "SELECT status,version::bigint version,kind,payload FROM iam_reviews \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.review_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if review.try_get::<String, _>("status")? != "pending"
            || review.try_get::<i64, _>("version")? != 1
            || review.try_get::<String, _>("kind")? != REVIEW_KIND_ROLE_PUBLISH
            || review.try_get::<Value, _>("payload")? != plan.review_payload
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_review(
        &mut self,
        actor: &IamRoleActor,
        plan: &ReviewDecisionPlan,
    ) -> Result<(), Error> {
        let review = sqlx::query(
            "SELECT status,version::bigint version FROM iam_reviews WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.review_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if review.try_get::<String, _>("status")? != plan.status()
            || review.try_get::<i64, _>("version")? != plan.expected_version + 1
        {
            return Err(readback_invalid().into());
        }
        if let Some(publication) = &plan.publication {
            let role = sqlx::query(
                "SELECT published_revision::bigint published_revision,name FROM iam_roles \
                 WHERE company_id=$1 AND id=$2",
            )
            .bind(actor.company_id.as_str())
            .bind(&publication.id)
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            if role.try_get::<i64, _>("published_revision")? != publication.revision
                || role.try_get::<String, _>("name")? != publication.name
            {
                return Err(readback_invalid().into());
            }
        }
        Ok(())
    }

    async fn readback_campaign_start(
        &mut self,
        actor: &IamRoleActor,
        plan: &AccessReviewStartPlan,
    ) -> Result<(), Error> {
        let campaign = sqlx::query(
            "SELECT status,version::bigint version FROM iam_access_review_campaigns \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.campaign_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        let items: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM iam_access_review_items WHERE campaign_id=$1 AND company_id=$2",
        )
        .bind(&plan.campaign_id)
        .bind(actor.company_id.as_str())
        .fetch_one(&mut *self.connection)
        .await?;
        let events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM iam_access_review_events WHERE id=$1 AND campaign_id=$2",
        )
        .bind(&plan.started_event_id)
        .bind(&plan.campaign_id)
        .fetch_one(&mut *self.connection)
        .await?;
        if campaign.try_get::<String, _>("status")? != "open"
            || campaign.try_get::<i64, _>("version")? != 1
            || items != plan.assignments.len() as i64
            || events != 1
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }

    async fn readback_campaign_decision(
        &mut self,
        actor: &IamRoleActor,
        plan: &AccessReviewDecisionPlan,
        campaign_status: &str,
    ) -> Result<(), Error> {
        let item = sqlx::query(
            "SELECT decision FROM iam_access_review_items WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.item_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if item.try_get::<String, _>("decision")? != plan.decision {
            return Err(readback_invalid().into());
        }
        let campaign = sqlx::query(
            "SELECT status,version::bigint version FROM iam_access_review_campaigns \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.campaign_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if campaign.try_get::<String, _>("status")? != campaign_status
            || campaign.try_get::<i64, _>("version")? != plan.resulting_campaign_version
        {
            return Err(readback_invalid().into());
        }
        if let Some(revocation) = &plan.revocation {
            let assignment = sqlx::query(
                "SELECT version,revoked_at FROM assignments WHERE id=$1 AND company_id=$2",
            )
            .bind(&revocation.assignment_id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?
            .ok_or_else(readback_invalid)?;
            if assignment.try_get::<i64, _>("version")? != revocation.assignment_version + 1
                || assignment.try_get::<Option<i64>, _>("revoked_at")? != Some(plan.now)
            {
                return Err(readback_invalid().into());
            }
            let remaining: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM active_work_contexts WHERE company_id=$1 AND assignment_id=$2",
            )
            .bind(actor.company_id.as_str())
            .bind(&revocation.assignment_id)
            .fetch_one(&mut *self.connection)
            .await?;
            if remaining != 0 {
                return Err(readback_invalid().into());
            }
        }
        Ok(())
    }

    async fn readback_campaign_cancel(
        &mut self,
        actor: &IamRoleActor,
        plan: &AccessReviewCancelPlan,
    ) -> Result<(), Error> {
        let campaign = sqlx::query(
            "SELECT status,version::bigint version FROM iam_access_review_campaigns \
             WHERE id=$1 AND company_id=$2",
        )
        .bind(&plan.campaign_id)
        .bind(actor.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?
        .ok_or_else(readback_invalid)?;
        if campaign.try_get::<String, _>("status")? != "cancelled"
            || campaign.try_get::<i64, _>("version")? != plan.resulting_version
        {
            return Err(readback_invalid().into());
        }
        Ok(())
    }
}

impl IamRolesReviewRepository for PgIamRolesReviews<'_> {
    type Error = Error;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        permission: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if actor.platform {
                self.platform_authority(actor, permission, now).await
            } else {
                self.holding_authority(actor, permission, now).await
            }
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> IamRolesFuture<'a, String, Self::Error> {
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
    ) -> IamRolesFuture<'a, Option<IamReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            if self.platform {
                let row = sqlx::query(
                    "SELECT request_hash,result,actor_id FROM iam_receipts WHERE id=$1",
                )
                .bind(key)
                .fetch_optional(&mut *self.connection)
                .await?;
                let snapshot = row
                    .map(|row| {
                        let envelope: Value = row.try_get("result")?;
                        let Some(operation) = envelope["operation"].as_str() else {
                            return Ok::<_, sqlx::Error>(None);
                        };
                        let Some(record_id) = envelope["recordId"].as_str() else {
                            return Ok(None);
                        };
                        let Some(result) = envelope.get("result") else {
                            return Ok(None);
                        };
                        Ok(Some(IamReceiptSnapshot {
                            request_hash: row.try_get("request_hash")?,
                            result_json: result.to_string(),
                            operation: operation.to_owned(),
                            actor_id: row.try_get("actor_id")?,
                            record_id: record_id.to_owned(),
                            authority_json: envelope["authority"].to_string(),
                        }))
                    })
                    .transpose()?;
                return Ok(snapshot.flatten());
            }
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id,authority_json \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, Error>(IamReceiptSnapshot {
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

    fn role_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<RoleRow>, Self::Error> {
        Box::pin(async move { self.role_row(actor, id, true).await })
    }

    fn next_revision<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT (COALESCE(MAX(revision),0)+1)::bigint next FROM iam_role_revisions \
                 WHERE company_id=$1 AND role_id=$2",
            )
            .bind(actor.company_id.as_str())
            .bind(id)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn affected_role_users<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Vec<String>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = if actor.platform {
                sqlx::query(
                    "SELECT DISTINCT user_id FROM platform_assignments WHERE role=$1 AND revoked_at IS NULL ORDER BY user_id",
                )
                .bind(id)
                .fetch_all(&mut *self.connection)
                .await?
            } else {
                sqlx::query(
                    "SELECT DISTINCT user_id FROM assignments WHERE role=$1 AND company_id=$2 \
                     AND revoked_at IS NULL ORDER BY user_id",
                )
                .bind(id)
                .bind(actor.company_id.as_str())
                .fetch_all(&mut *self.connection)
                .await?
            };
            rows.into_iter()
                .map(|row| row.try_get::<String, _>("user_id").map_err(Into::into))
                .collect()
        })
    }

    fn commit_role_save<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a RoleSavePlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            if plan.created {
                sqlx::query(
                    "INSERT INTO iam_roles(company_id,id,name) VALUES($1,$2,$3) \
                     ON CONFLICT(company_id,id) DO NOTHING",
                )
                .bind(actor.company_id.as_str())
                .bind(&plan.id)
                .bind(&plan.name)
                .execute(&mut *self.connection)
                .await?;
            }
            let inserted = sqlx::query(
                "INSERT INTO iam_role_revisions\
                 (company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) \
                 VALUES($1,$2,$3,$4::jsonb,$5::jsonb,$6::jsonb,$7,$8) ON CONFLICT DO NOTHING",
            )
            .bind(actor.company_id.as_str())
            .bind(&plan.id)
            .bind(plan.revision)
            .bind(serde_json::to_string(&plan.permissions).map_err(|_| source_invalid())?)
            .bind(
                serde_json::to_string(&plan.grantable_permissions).map_err(|_| source_invalid())?,
            )
            .bind(serde_json::to_string(&plan.scopes).map_err(|_| source_invalid())?)
            .bind(actor.user_id.as_str())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(readback_invalid().into());
            }
            let created_person = actor
                .person_id
                .as_ref()
                .map(|value| value.as_str())
                .unwrap_or_else(|| actor.user_id.as_str());
            let inserted = sqlx::query(
                "INSERT INTO iam_reviews\
                 (id,company_id,kind,payload,created_by,created_person,created_at,status,version,reason) \
                 VALUES($1,$2,$3,$4::jsonb,$5,$6,$7,'pending',1,$8) ON CONFLICT DO NOTHING",
            )
            .bind(&plan.review_id)
            .bind(actor.company_id.as_str())
            .bind(REVIEW_KIND_ROLE_PUBLISH)
            .bind(plan.review_payload.to_string())
            .bind(actor.user_id.as_str())
            .bind(created_person)
            .bind(plan.now)
            .bind(&plan.reason)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if inserted != 1 {
                return Err(readback_invalid().into());
            }
            let result = plan.receipt_json();
            self.effects_with_hash(
                actor,
                key,
                request_hash,
                ROLE_SAVE_OPERATION,
                "iam_role_review",
                &plan.review_id,
                1,
                &result,
                &plan.audit_evidence(),
                &plan.outbox_payload(
                    serde_json::from_str(&actor.authority_json())
                        .map_err(|_| readback_invalid())?,
                ),
                plan.now,
            )
            .await?;
            self.readback_role(actor, plan).await?;
            Ok(serde_json::from_str(&result).map_err(|_| readback_invalid())?)
        })
    }

    fn review_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewRow>, Self::Error> {
        Box::pin(async move { self.review_row(actor, id).await })
    }

    fn same_person<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        user_id: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
                .bind(user_id)
                .bind(actor.user_id.as_str())
                .bind(actor.company_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn branch_covers<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        node_id: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$2 \
                 AND branch_id=$3)",
            )
            .bind(node_id)
            .bind(actor.company_id.as_str())
            .bind(actor.scope_node_id.as_ref().map(|value| value.as_str()))
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn proposer_authority<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        assignment_id: &'a str,
        proposer_user_id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, Option<ProposerAuthority>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = if actor.platform {
                sqlx::query(
                    "SELECT a.role,v.grantable_json::text grantable_json FROM platform_assignments a \
                     JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',$2) AND r.status='active' \
                     JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
                      AND v.revision=r.published_revision \
                     WHERE a.id=$1 AND a.user_id=$3 AND a.revoked_at IS NULL \
                      AND iam_assignment_active(a.id,$4)",
                )
                .bind(assignment_id)
                .bind(actor.company_id.as_str())
                .bind(proposer_user_id)
                .bind(now)
                .fetch_optional(&mut *self.connection)
                .await?
            } else {
                sqlx::query(
                    "SELECT a.role,v.grantable_json::text grantable_json FROM assignments a \
                     JOIN iam_roles r ON r.id=a.role AND r.company_id IN ('*',$2) AND r.status='active' \
                     JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id \
                      AND v.revision=r.published_revision \
                     WHERE a.id=$1 AND a.user_id=$3 AND a.company_id=$2 AND a.revoked_at IS NULL \
                      AND iam_assignment_active(a.id,$4) AND iam_principal_active(a.user_id,$2,$4)",
                )
                .bind(assignment_id)
                .bind(actor.company_id.as_str())
                .bind(proposer_user_id)
                .bind(now)
                .fetch_optional(&mut *self.connection)
                .await?
            };
            row.map(|row| {
                Ok::<_, Error>(ProposerAuthority {
                    role: row.try_get("role")?,
                    grantable_permissions: parse_strings(row.try_get("grantable_json")?)?,
                })
            })
            .transpose()
        })
    }

    fn role_allows<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        role: &'a str,
        permission: &'a str,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar("SELECT iam_role_allows($1,$2,$3)")
                .bind(role)
                .bind(actor.company_id.as_str())
                .bind(permission)
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn reviewer_affected_by_role<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        role_id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let query = if actor.platform {
                "SELECT EXISTS(SELECT 1 FROM platform_assignments WHERE role=$1 \
                 AND iam_assignment_active(id,$2) AND iam_same_person(user_id,$3,$4))"
            } else {
                "SELECT EXISTS(SELECT 1 FROM assignments WHERE role=$1 AND company_id=$4 \
                 AND iam_assignment_active(id,$2) AND iam_same_person(user_id,$3,$4))"
            };
            Ok(sqlx::query_scalar(query)
                .bind(role_id)
                .bind(now)
                .bind(actor.user_id.as_str())
                .bind(actor.company_id.as_str())
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn commit_review_decision<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a ReviewDecisionPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            if let Some(publication) = &plan.publication {
                let role = sqlx::query(
                    "SELECT built_in,published_revision::bigint published_revision FROM iam_roles \
                     WHERE company_id=$1 AND id=$2 FOR UPDATE",
                )
                .bind(actor.company_id.as_str())
                .bind(&publication.id)
                .fetch_optional(&mut *self.connection)
                .await?
                .ok_or_else(role_changed)?;
                if role.try_get::<bool, _>("built_in")?
                    || role.try_get::<i64, _>("published_revision")? != publication.expected_version
                {
                    return Err(role_changed().into());
                }
                sqlx::query(
                    "UPDATE iam_roles SET published_revision=$1,name=$2 WHERE company_id=$3 AND id=$4",
                )
                .bind(publication.revision)
                .bind(&publication.name)
                .bind(actor.company_id.as_str())
                .bind(&publication.id)
                .execute(&mut *self.connection)
                .await?;
                if actor.platform {
                    sqlx::query("UPDATE platform_assignments SET version=version+1 WHERE role=$1")
                        .bind(&publication.id)
                        .execute(&mut *self.connection)
                        .await?;
                } else {
                    sqlx::query(
                        "UPDATE assignments SET version=version+1 WHERE role=$1 AND company_id=$2",
                    )
                    .bind(&publication.id)
                    .bind(actor.company_id.as_str())
                    .execute(&mut *self.connection)
                    .await?;
                    sqlx::query(
                        "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,\
                         revision=revision+1,updated_at=$2 WHERE company_id=$1 AND assignment_id IN \
                         (SELECT id FROM assignments WHERE role=$3 AND company_id=$1)",
                    )
                    .bind(actor.company_id.as_str())
                    .bind(plan.now)
                    .bind(&publication.id)
                    .execute(&mut *self.connection)
                    .await?;
                }
            }
            let changed = sqlx::query(
                "UPDATE iam_reviews SET status=$1,reviewed_by=$2,reviewed_at=$3,\
                 reason=reason||$4,version=version+1 \
                 WHERE id=$5 AND company_id=$6 AND status='pending' AND version=$7",
            )
            .bind(plan.status())
            .bind(actor.user_id.as_str())
            .bind(plan.now)
            .bind(format!("\nReview: {}", plan.reason))
            .bind(&plan.review_id)
            .bind(actor.company_id.as_str())
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(review_changed().into());
            }
            let result = plan.receipt_json();
            self.effects_with_hash(
                actor,
                key,
                request_hash,
                REVIEW_DECIDE_OPERATION,
                "iam_review",
                &plan.review_id,
                plan.expected_version + 1,
                &result,
                &plan.audit_evidence(),
                &plan.outbox_payload(
                    serde_json::from_str(&actor.authority_json())
                        .map_err(|_| readback_invalid())?,
                ),
                plan.now,
            )
            .await?;
            self.readback_review(actor, plan).await?;
            Ok(serde_json::from_str(&result).map_err(|_| readback_invalid())?)
        })
    }

    fn active_review_assignments<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        now: i64,
    ) -> IamRolesFuture<'a, Vec<ReviewAssignment>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let rows = sqlx::query(
                "SELECT id,user_id,role,scope_kind,scope_node_id,source,valid_until,version,revoked_at \
                 FROM assignments WHERE company_id=$1 AND revoked_at IS NULL \
                 AND iam_assignment_active(id,$2) AND iam_principal_active(user_id,$1,$2) \
                 ORDER BY user_id,role,id LIMIT 1001",
            )
            .bind(actor.company_id.as_str())
            .bind(now)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(ReviewAssignment {
                        id: row.try_get("id")?,
                        user_id: row.try_get("user_id")?,
                        role: row.try_get("role")?,
                        scope_kind: row.try_get("scope_kind")?,
                        scope_node_id: row.try_get("scope_node_id")?,
                        source: row.try_get("source")?,
                        valid_until: row.try_get("valid_until")?,
                        version: row.try_get("version")?,
                        revoked_at: row.try_get("revoked_at")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()
                .map_err(Into::into)
        })
    }

    fn commit_campaign_start<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a AccessReviewStartPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            sqlx::query(
                "INSERT INTO iam_access_review_campaigns\
                 (id,company_id,name,due_at,status,version,created_by,reason,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,'open',1,$5,$6,$7,$7) ON CONFLICT DO NOTHING",
            )
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .bind(&plan.name)
            .bind(plan.due_at)
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            for (index, assignment) in plan.assignments.iter().enumerate() {
                let item_id = crate::domain::iam_roles::campaign_item_id(key, index);
                sqlx::query(
                    "INSERT INTO iam_access_review_items\
                     (id,campaign_id,company_id,assignment_id,assignment_version,user_id,role,\
                     scope_kind,scope_node_id,source,valid_until) \
                     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING",
                )
                .bind(item_id)
                .bind(&plan.campaign_id)
                .bind(actor.company_id.as_str())
                .bind(&assignment.id)
                .bind(assignment.version)
                .bind(&assignment.user_id)
                .bind(&assignment.role)
                .bind(&assignment.scope_kind)
                .bind(&assignment.scope_node_id)
                .bind(&assignment.source)
                .bind(assignment.valid_until)
                .execute(&mut *self.connection)
                .await?;
            }
            let snapshot = json!({
                "name": plan.name,
                "dueAt": plan.due_at,
                "reason": plan.reason,
                "assignmentIds": plan.assignments.iter().map(|a| &a.id).collect::<Vec<_>>(),
            });
            sqlx::query(
                "INSERT INTO iam_access_review_events\
                 (id,campaign_id,company_id,action,actor_id,snapshot_json,created_at) \
                 VALUES($1,$2,$3,'started',$4,$5,$6)",
            )
            .bind(&plan.started_event_id)
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(snapshot.to_string())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let result = plan.receipt_json();
            self.effects_with_hash(
                actor,
                key,
                request_hash,
                ACCESS_REVIEW_START_OPERATION,
                "iam_access_review",
                &plan.campaign_id,
                1,
                &result,
                &plan.audit_evidence(),
                &plan.outbox_payload(
                    serde_json::from_str(&actor.authority_json())
                        .map_err(|_| readback_invalid())?,
                ),
                plan.now,
            )
            .await?;
            self.readback_campaign_start(actor, plan).await?;
            Ok(serde_json::from_str(&result).map_err(|_| readback_invalid())?)
        })
    }

    fn review_item_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        item_id: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewItemRow>, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let row = sqlx::query(
                "SELECT i.id,i.campaign_id,i.company_id,i.assignment_id,\
                 i.assignment_version::bigint assignment_version,i.user_id,i.role,i.scope_kind,\
                 i.scope_node_id,i.source,i.valid_until,i.decision,c.status campaign_status,\
                 c.version::bigint campaign_version,c.created_by \
                 FROM iam_access_review_items i \
                 JOIN iam_access_review_campaigns c ON c.id=i.campaign_id \
                 WHERE i.id=$1 AND i.company_id=$2 FOR UPDATE OF i,c",
            )
            .bind(item_id)
            .bind(actor.company_id.as_str())
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<ReviewItemRow, sqlx::Error>(ReviewItemRow {
                    id: row.try_get("id")?,
                    campaign_id: row.try_get("campaign_id")?,
                    company_id: row.try_get("company_id")?,
                    assignment_id: row.try_get("assignment_id")?,
                    assignment_version: row.try_get("assignment_version")?,
                    user_id: row.try_get("user_id")?,
                    role: row.try_get("role")?,
                    scope_kind: row.try_get("scope_kind")?,
                    scope_node_id: row.try_get("scope_node_id")?,
                    source: row.try_get("source")?,
                    valid_until: row.try_get("valid_until")?,
                    decision: row.try_get("decision")?,
                    campaign_status: row.try_get("campaign_status")?,
                    campaign_version: row.try_get("campaign_version")?,
                    created_by: row.try_get("created_by")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn assignment_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<ReviewAssignment>, Self::Error> {
        Box::pin(async move { self.assignment_row(actor, id).await })
    }

    fn assignment_active<'a>(
        &'a mut self,
        _actor: &'a IamRoleActor,
        id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar("SELECT iam_assignment_active($1,$2)")
                .bind(id)
                .bind(now)
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn revocation_guard<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        user_id: &'a str,
        now: i64,
    ) -> IamRolesFuture<'a, RevocationGuardFacts, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            let facts = sqlx::query(
                "SELECT iam_same_person($1,$2,$3) same_person,\
                 EXISTS(SELECT 1 FROM iam_members m WHERE m.company_id=$3 AND m.user_id=$1 \
                  AND m.login_policy='emergency' AND m.status='active') recovery_member,\
                 EXISTS(SELECT 1 FROM iam_members m JOIN assignments a ON a.user_id=m.user_id \
                  AND a.company_id=m.company_id WHERE m.company_id=$3 AND m.user_id<>$1 \
                  AND m.login_policy='emergency' AND m.status='active' AND a.role='holding_owner' \
                  AND a.revoked_at IS NULL AND iam_assignment_active(a.id,$4) \
                  AND iam_principal_active(a.user_id,a.company_id,$4)) other_recovery_administrator,\
                 EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=$3 AND a.user_id<>$1 \
                  AND a.role='holding_owner' AND a.revoked_at IS NULL \
                  AND iam_assignment_active(a.id,$4) \
                  AND iam_principal_active(a.user_id,a.company_id,$4)) other_administrator",
            )
            .bind(user_id)
            .bind(actor.user_id.as_str())
            .bind(actor.company_id.as_str())
            .bind(now)
            .fetch_one(&mut *self.connection)
            .await?;
            Ok::<RevocationGuardFacts, Error>(RevocationGuardFacts {
                same_person: facts.try_get("same_person")?,
                recovery_member: facts.try_get("recovery_member")?,
                other_recovery_administrator: facts.try_get("other_recovery_administrator")?,
                other_administrator: facts.try_get("other_administrator")?,
            })
        })
    }

    fn commit_campaign_decision<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a AccessReviewDecisionPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            if let Some(revocation) = &plan.revocation {
                let changed = sqlx::query(
                    "UPDATE assignments SET revoked_at=$1,version=version+1 \
                     WHERE id=$2 AND company_id=$3 AND version=$4 AND revoked_at IS NULL",
                )
                .bind(plan.now)
                .bind(&revocation.assignment_id)
                .bind(actor.company_id.as_str())
                .bind(revocation.assignment_version)
                .execute(&mut *self.connection)
                .await?
                .rows_affected();
                if changed != 1 {
                    return Err(review_source_changed().into());
                }
                sqlx::query(
                    "UPDATE active_work_contexts SET assignment_id=NULL,assignment_version=NULL,\
                     revision=revision+1,updated_at=$3 WHERE company_id=$1 AND assignment_id=$2",
                )
                .bind(actor.company_id.as_str())
                .bind(&revocation.assignment_id)
                .bind(plan.now)
                .execute(&mut *self.connection)
                .await?;
            }
            let changed = sqlx::query(
                "UPDATE iam_access_review_items SET decision=$1,decided_by=$2,decision_reason=$3,\
                 decided_at=$4 WHERE id=$5 AND campaign_id=$6 AND company_id=$7 \
                 AND decision='pending'",
            )
            .bind(&plan.decision)
            .bind(actor.user_id.as_str())
            .bind(&plan.reason)
            .bind(plan.now)
            .bind(&plan.item_id)
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(access_review_item_changed().into());
            }
            let event = if plan.revocation.is_some() {
                "revoked"
            } else {
                "kept"
            };
            let snapshot = json!({
                "decision": plan.decision,
                "reason": plan.reason,
                "itemId": plan.item_id,
                "campaignVersion": plan.resulting_campaign_version,
            });
            sqlx::query(
                "INSERT INTO iam_access_review_events\
                 (id,campaign_id,item_id,company_id,action,actor_id,snapshot_json,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&plan.decision_event_id)
            .bind(&plan.campaign_id)
            .bind(&plan.item_id)
            .bind(actor.company_id.as_str())
            .bind(event)
            .bind(actor.user_id.as_str())
            .bind(snapshot.to_string())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let remaining: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM iam_access_review_items \
                 WHERE campaign_id=$1 AND company_id=$2 AND decision='pending'",
            )
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
            let status = if remaining == 0 { "completed" } else { "open" };
            let changed = sqlx::query(
                "UPDATE iam_access_review_campaigns SET status=$1,version=$2,\
                 completed_at=$3,updated_at=$4 WHERE id=$5 AND company_id=$6 \
                 AND status='open' AND version=$7",
            )
            .bind(status)
            .bind(plan.resulting_campaign_version)
            .bind((remaining == 0).then_some(plan.now))
            .bind(plan.now)
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .bind(plan.expected_campaign_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(access_review_campaign_changed().into());
            }
            if remaining == 0 {
                if let Some(event_id) = &plan.completed_event_id {
                    sqlx::query(
                        "INSERT INTO iam_access_review_events\
                         (id,campaign_id,company_id,action,actor_id,snapshot_json,created_at) \
                         VALUES($1,$2,$3,'completed',$4,$5,$6)",
                    )
                    .bind(event_id)
                    .bind(&plan.campaign_id)
                    .bind(actor.company_id.as_str())
                    .bind(actor.user_id.as_str())
                    .bind(json!({"completedItems": true}).to_string())
                    .bind(plan.now)
                    .execute(&mut *self.connection)
                    .await?;
                }
            }
            let result = plan.receipt_json(status);
            self.effects_with_hash(
                actor,
                key,
                request_hash,
                ACCESS_REVIEW_DECIDE_OPERATION,
                "iam_access_review_item",
                &plan.item_id,
                plan.resulting_campaign_version,
                &result,
                &plan.audit_evidence(status),
                &plan.outbox_payload(
                    serde_json::from_str(&actor.authority_json())
                        .map_err(|_| readback_invalid())?,
                    status,
                ),
                plan.now,
            )
            .await?;
            self.readback_campaign_decision(actor, plan, status).await?;
            Ok(serde_json::from_str(&result).map_err(|_| readback_invalid())?)
        })
    }

    fn campaign_for_update<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        id: &'a str,
    ) -> IamRolesFuture<'a, Option<CampaignRow>, Self::Error> {
        Box::pin(async move { self.campaign_row(actor, id).await })
    }

    fn decided_items<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        campaign_id: &'a str,
    ) -> IamRolesFuture<'a, i64, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            Ok(sqlx::query_scalar(
                "SELECT count(*) FROM iam_access_review_items \
                 WHERE campaign_id=$1 AND company_id=$2 AND decision!='pending'",
            )
            .bind(campaign_id)
            .bind(actor.company_id.as_str())
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn commit_campaign_cancel<'a>(
        &'a mut self,
        actor: &'a IamRoleActor,
        key: &'a str,
        plan: &'a AccessReviewCancelPlan,
        request_hash: &'a str,
    ) -> IamRolesFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.ready().await?;
            self.lock_command(key).await?;
            let changed = sqlx::query(
                "UPDATE iam_access_review_campaigns SET status='cancelled',version=$1,updated_at=$2 \
                 WHERE id=$3 AND company_id=$4 AND status='open' AND version=$5",
            )
            .bind(plan.resulting_version)
            .bind(plan.now)
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .bind(plan.expected_version)
            .execute(&mut *self.connection)
            .await?
            .rows_affected();
            if changed != 1 {
                return Err(access_review_campaign_changed().into());
            }
            sqlx::query(
                "INSERT INTO iam_access_review_events\
                 (id,campaign_id,company_id,action,actor_id,snapshot_json,created_at) \
                 VALUES($1,$2,$3,'cancelled',$4,$5,$6)",
            )
            .bind(&plan.cancelled_event_id)
            .bind(&plan.campaign_id)
            .bind(actor.company_id.as_str())
            .bind(actor.user_id.as_str())
            .bind(json!({"reason": plan.reason}).to_string())
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await?;
            let result = plan.receipt_json();
            self.effects_with_hash(
                actor,
                key,
                request_hash,
                ACCESS_REVIEW_CANCEL_OPERATION,
                "iam_access_review",
                &plan.campaign_id,
                plan.resulting_version,
                &result,
                &plan.audit_evidence(),
                &plan.outbox_payload(
                    serde_json::from_str(&actor.authority_json())
                        .map_err(|_| readback_invalid())?,
                ),
                plan.now,
            )
            .await?;
            self.readback_campaign_cancel(actor, plan).await?;
            Ok(serde_json::from_str(&result).map_err(|_| readback_invalid())?)
        })
    }
}
