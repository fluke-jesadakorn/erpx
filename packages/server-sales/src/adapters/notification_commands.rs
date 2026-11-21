//! PostgreSQL adapter for native Sales customer-notification commands.
//!
//! The caller owns the retained serializable Holding transaction. The adapter
//! re-reads the actual Sales source/customer/settings facts, computes the exact
//! delivery-worker source hash, writes settings/notice/authorization and the
//! receipt/audit/outbox unit, flushes deferred effects and then re-checks the
//! current actor, capability and exact readback.
use crate::adapters::sales_notification_execution::{RELEASE_READY, SOURCE_IMAGE};
use crate::application::notification_commands::{
    NotificationCommandRepository, NotificationFuture, NotificationPurpose, QueuePlan,
    RetryPlan, SettingsPlan,
};
use crate::domain::notifications::{
    DeliveryFacts, NotificationActor, NotificationReceiptSnapshot, NoticeSource, SettingsAdapter,
    access_changed, feature_unavailable, readback_invalid, source_changed, version_conflict,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PersistenceError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments g \
 WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
 AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
 AND iam_role_allows(g.role,g.company_id,$6) \
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$7 \
   OR g.scope_kind='department' AND g.scope_node_id=$8) \
 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r \
   WHERE r.company_id=$4 AND r.user_id=$2 AND r.session_id=$9) \
 AND ($10::bigint IS NULL OR EXISTS(SELECT 1 FROM assignments a WHERE a.id=$1 AND a.version=$10)))";

const FEATURE: &str = "SELECT (NOT $2::boolean OR EXISTS(SELECT 1 FROM engineering_verification_contexts e \
 WHERE e.company_id=$1 AND e.enabled AND e.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))";

const DELIVERY: &str = "SELECT f.id fulfillment_id,f.version fulfillment_version,f.number fulfillment_number,\
 f.fulfillment_date,o.id order_id,o.number order_number,o.branch_id,o.department_id,\
 p.id party_id,p.name party_name,p.version party_version,p.notification_email,p.notification_consent,\
 ns.adapter,ns.version settings_version \
 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id \
 JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id \
 LEFT JOIN commerce_customer_notification_settings ns ON ns.company_id=o.company_id \
 WHERE f.id=$1 AND o.kind='sales' AND o.company_id=$2 \
 AND ($3::text IS NULL OR o.branch_id=$3) AND ($4::text IS NULL OR o.department_id=$4)";

pub struct PgNotificationCommands<'c> {
    connection: &'c mut PgConnection,
    mode: PackageEnforcementMode,
    production: bool,
}

impl<'c> PgNotificationCommands<'c> {
    pub fn new(
        connection: &'c mut PgConnection,
        mode: PackageEnforcementMode,
        production: bool,
    ) -> Self {
        Self {
            connection,
            mode,
            production,
        }
    }

    fn ready(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("SALES_NOTIFICATION_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }

    async fn load_notice(&mut self, id: &str) -> Result<Option<NoticeSource>, Error> {
        let sql = format!(
            "SELECT n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,\
             n.recipient_email,n.subject,n.body_text,n.adapter,n.status,n.version,n.attempts,\
             n.provider_message_id,n.created_by,delivery.department_id,delivery.order_state,\
             delivery.company_id source_company,customer.company_id customer_company,\
             customer.status customer_status,customer.notification_email,customer.notification_consent,\
             settings.adapter current_adapter,{RELEASE_READY} release_ready,\
             encode(sha256(convert_to({SOURCE_IMAGE},'UTF8')),'hex') source_hash \
             FROM commerce_customer_notifications n \
             LEFT JOIN (SELECT f.id fulfillment_id,o.id order_id,o.company_id,o.branch_id,\
               o.department_id,o.kind order_kind,o.state order_state,o.currency,o.party_id \
               FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales') delivery \
               ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id \
               AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id \
             LEFT JOIN (SELECT id,company_id,kind,version,status,notification_email,\
               notification_consent FROM finance_parties WHERE kind='customer') customer \
               ON customer.id=n.party_id AND customer.company_id=n.company_id \
             LEFT JOIN commerce_customer_notification_settings settings \
               ON settings.company_id=n.company_id \
             WHERE n.id=$1"
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(self.production)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok::<_, sqlx::Error>(NoticeSource {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                fulfillment_id: row.try_get("fulfillment_id")?,
                order_id: row.try_get("order_id")?,
                party_id: row.try_get("party_id")?,
                recipient_email: row.try_get("recipient_email")?,
                subject: row.try_get("subject")?,
                body_text: row.try_get("body_text")?,
                adapter: row.try_get("adapter")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                attempts: row.try_get("attempts")?,
                provider_message_id: row.try_get("provider_message_id")?,
                created_by: row.try_get("created_by")?,
                order_state: row.try_get("order_state")?,
                customer_status: row.try_get("customer_status")?,
                notification_email: row.try_get("notification_email")?,
                notification_consent: row.try_get("notification_consent")?,
                current_adapter: row.try_get("current_adapter")?,
                source_company: row.try_get("source_company")?,
                customer_company: row.try_get("customer_company")?,
                source_hash: row.try_get("source_hash")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }

    async fn authority(
        &mut self,
        actor: &NotificationActor,
        permission: &str,
        now: i64,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar(AUTHORITY)
            .bind(&actor.assignment_id)
            .bind(&actor.user_id)
            .bind(&actor.role)
            .bind(&actor.company_id)
            .bind(now)
            .bind(permission)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.session_id.as_deref())
            .bind(actor.assignment_version)
            .fetch_one(&mut *self.connection)
            .await?)
    }

    async fn capability(
        &mut self,
        company_id: &str,
        purpose: NotificationPurpose,
        now: i64,
    ) -> Result<(), Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Ok(());
        }
        let catalogue = current_package_catalogue()?;
        let company = CompanyId::new(company_id)?;
        let purpose = match purpose {
            NotificationPurpose::NewWork => CapabilityPurpose::NewWork,
            NotificationPurpose::SettleExisting => CapabilityPurpose::SettleExisting,
            NotificationPurpose::History => CapabilityPurpose::History,
        };
        PgCapabilityDecisionReader::new(company, self.connection, &catalogue)
            .require("sales.orders", purpose, self.mode, TimestampMillis::new(now))
            .await?;
        Ok(())
    }

    async fn ledger(
        &mut self,
        actor: &NotificationActor,
        key: &str,
        operation: &str,
        request_hash: &str,
        record_id: &str,
        result: &str,
        version: i64,
        purpose: &str,
        now: i64,
    ) -> Result<(), Error> {
        let authority = actor.authority_json();
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,\
             result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(key)
        .bind(operation)
        .bind(&actor.user_id)
        .bind(request_hash)
        .bind(record_id)
        .bind(result)
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,\
             record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,'commerce_customer_notification',$6,$7,$8,$9,$10)",
        )
        .bind(format!("audit-notification-{key}"))
        .bind(key)
        .bind(&actor.user_id)
        .bind(&actor.assignment_id)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(purpose)
        .bind(now)
        .bind(&authority)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
             aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,'commerce_customer_notification',$4,$5,$6,$7)",
        )
        .bind(format!("outbox-notification-{key}"))
        .bind(key)
        .bind(operation)
        .bind(record_id)
        .bind(version)
        .bind(result)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<(), Error> {
        sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
}

impl NotificationCommandRepository for PgNotificationCommands<'_> {
    type Error = Error;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> NotificationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("sales-notification-command:{key}"))
                .execute(&mut *self.connection)
                .await?;
            Ok(())
        })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> NotificationFuture<'a, String, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_scalar(
                "SELECT encode(sha256(convert_to($1,'UTF8')),'hex')",
            )
            .bind(value)
            .fetch_one(&mut *self.connection)
            .await?)
        })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> NotificationFuture<'a, Option<NotificationReceiptSnapshot>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(
                "SELECT request_hash,result_json,operation,actor_id,record_id \
                 FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(NotificationReceiptSnapshot {
                    request_hash: row.try_get("request_hash")?,
                    result_json: row.try_get("result_json")?,
                    operation: row.try_get("operation")?,
                    actor_id: row.try_get("actor_id")?,
                    record_id: row.try_get("record_id")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn settings<'a>(
        &'a mut self,
        company_id: &'a str,
    ) -> NotificationFuture<'a, Option<(String, i64)>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_as::<_, (String, i64)>(
                "SELECT adapter,version FROM commerce_customer_notification_settings WHERE company_id=$1",
            )
            .bind(company_id)
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn delivery<'a>(
        &'a mut self,
        actor: &'a NotificationActor,
        fulfillment_id: &'a str,
    ) -> NotificationFuture<'a, Option<DeliveryFacts>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let row = sqlx::query(sqlx::AssertSqlSafe(DELIVERY.to_owned()))
                .bind(fulfillment_id)
                .bind(&actor.company_id)
                .bind(actor.branch_id.as_deref())
                .bind(actor.department_id.as_deref())
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(DeliveryFacts {
                    fulfillment_id: row.try_get("fulfillment_id")?,
                    fulfillment_version: row.try_get("fulfillment_version")?,
                    fulfillment_number: row.try_get("fulfillment_number")?,
                    fulfillment_date: row.try_get("fulfillment_date")?,
                    order_id: row.try_get("order_id")?,
                    order_number: row.try_get("order_number")?,
                    branch_id: row.try_get("branch_id")?,
                    department_id: row.try_get("department_id")?,
                    party_id: row.try_get("party_id")?,
                    party_name: row.try_get("party_name")?,
                    party_version: row.try_get("party_version")?,
                    notification_email: row.try_get("notification_email")?,
                    notification_consent: row.try_get("notification_consent")?,
                    adapter: row.try_get("adapter")?,
                    settings_version: row.try_get("settings_version")?,
                })
            })
            .transpose()
            .map_err(Into::into)
        })
    }

    fn delivery_lines<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> NotificationFuture<'a, Vec<(String, i64, String)>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let rows = sqlx::query(
                "SELECT p.sku,fl.quantity_milli,p.unit FROM commerce_fulfillment_lines fl \
                 JOIN commerce_order_lines ol ON ol.id=fl.order_line_id \
                 JOIN commerce_products p ON p.id=ol.product_id \
                 WHERE fl.fulfillment_id=$1 ORDER BY ol.ordinal",
            )
            .bind(fulfillment_id)
            .fetch_all(&mut *self.connection)
            .await?;
            rows.into_iter()
                .map(|row| {
                    Ok::<_, sqlx::Error>((
                        row.try_get::<String, _>("sku")?,
                        row.try_get::<i64, _>("quantity_milli")?,
                        row.try_get::<String, _>("unit")?,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(Into::into)
        })
    }

    fn existing<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> NotificationFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_scalar(
                "SELECT status FROM commerce_customer_notifications WHERE fulfillment_id=$1 LIMIT 1",
            )
            .bind(fulfillment_id)
            .fetch_optional(&mut *self.connection)
            .await?)
        })
    }

    fn notice<'a>(
        &'a mut self,
        id: &'a str,
    ) -> NotificationFuture<'a, Option<NoticeSource>, Self::Error> {
        Box::pin(async move { self.load_notice(id).await })
    }

    fn feature_ready<'a>(
        &'a mut self,
        company_id: &'a str,
        _: i64,
    ) -> NotificationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.ready()?;
            Ok(sqlx::query_scalar(FEATURE)
                .bind(company_id)
                .bind(self.production)
                .fetch_one(&mut *self.connection)
                .await?)
        })
    }

    fn capability<'a>(
        &'a mut self,
        company_id: &'a str,
        purpose: NotificationPurpose,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        Box::pin(async move { self.capability(company_id, purpose, now).await })
    }

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a NotificationActor,
        permission: &'a str,
        now: i64,
    ) -> NotificationFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.authority(actor, permission, now).await })
    }

    fn commit_settings<'a>(
        &'a mut self,
        plan: &'a SettingsPlan<'a>,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query(
                "INSERT INTO commerce_customer_notification_settings(company_id,adapter,version,updated_by,updated_at) \
                 VALUES($1,$2,1,$3,$4) ON CONFLICT(company_id) DO UPDATE SET adapter=excluded.adapter,\
                 version=commerce_customer_notification_settings.version+1,updated_by=excluded.updated_by,\
                 updated_at=excluded.updated_at",
            )
            .bind(&plan.actor.company_id)
            .bind(plan.input.adapter.as_str())
            .bind(&plan.actor.user_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            let result = plan.receipt.wire().to_string();
            self.ledger(
                plan.actor,
                plan.key,
                crate::domain::notifications::SETTINGS_OPERATION,
                plan.request_hash,
                &plan.actor.company_id,
                &result,
                plan.receipt.version,
                "Customer notification settings",
                now,
            )
            .await?;
            self.flush().await?;
            if !self
                .authority(plan.actor, "commerce.approve", now)
                .await?
            {
                return Err(access_changed().into());
            }
            let purpose = if plan.input.adapter == SettingsAdapter::Disabled {
                NotificationPurpose::SettleExisting
            } else {
                NotificationPurpose::NewWork
            };
            self.capability(&plan.actor.company_id, purpose, now)
                .await?;
            let actual: Option<(String, i64)> = sqlx::query_as(
                "SELECT adapter,version FROM commerce_customer_notification_settings WHERE company_id=$1",
            )
            .bind(&plan.actor.company_id)
            .fetch_optional(&mut *self.connection)
            .await?;
            if actual != Some((plan.receipt.adapter.clone(), plan.receipt.version)) {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }

    fn commit_queue<'a>(
        &'a mut self,
        plan: &'a QueuePlan<'a>,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            sqlx::query(
                "INSERT INTO commerce_customer_notifications(id,company_id,branch_id,order_id,\
                 fulfillment_id,party_id,party_version,recipient_email,subject,body_text,adapter,\
                 next_attempt_at,created_by,created_at,updated_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$14)",
            )
            .bind(plan.id)
            .bind(&plan.actor.company_id)
            .bind(&plan.facts.branch_id)
            .bind(&plan.facts.order_id)
            .bind(plan.fulfillment_id)
            .bind(&plan.facts.party_id)
            .bind(plan.facts.party_version)
            .bind(plan.recipient_email)
            .bind(plan.subject)
            .bind(plan.body)
            .bind(plan.facts.adapter.as_deref().unwrap_or("disabled"))
            .bind(now)
            .bind(&plan.actor.user_id)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            let source = self
                .load_notice(plan.id)
                .await?
                .ok_or_else(readback_invalid)?;
            if source.invalid_reason().is_some() || source.status != "pending" || source.version != 1 {
                return Err(source_changed().into());
            }
            let authority = plan.actor.authority_json();
            sqlx::query(
                "INSERT INTO sales_notification_execution_authorizations(command_id,notification_id,\
                 company_id,kind,actor_id,assignment_id,authority_json,source_hash,notification_version,created_at) \
                 VALUES($1,$2,$3,'queue',$4,$5,$6,$7,1,$8)",
            )
            .bind(plan.key)
            .bind(plan.id)
            .bind(&plan.actor.company_id)
            .bind(&plan.actor.user_id)
            .bind(&plan.actor.assignment_id)
            .bind(&authority)
            .bind(&source.source_hash)
            .bind(now)
            .execute(&mut *self.connection)
            .await?;
            let result = plan.receipt.wire().to_string();
            self.ledger(
                plan.actor,
                plan.key,
                crate::domain::notifications::QUEUE_OPERATION,
                plan.request_hash,
                plan.fulfillment_id,
                &result,
                1,
                "Customer notification queued",
                now,
            )
            .await?;
            self.flush().await?;
            if !self.authority(plan.actor, "commerce.sales", now).await? {
                return Err(access_changed().into());
            }
            self.capability(
                &plan.actor.company_id,
                NotificationPurpose::NewWork,
                now,
            )
            .await?;
            let final_source = self
                .load_notice(plan.id)
                .await?
                .ok_or_else(readback_invalid)?;
            let authorization: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sales_notification_execution_authorizations \
                 WHERE command_id=$1 AND notification_id=$2 AND company_id=$3 \
                   AND source_hash=$4 AND notification_version=1)",
            )
            .bind(plan.key)
            .bind(plan.id)
            .bind(&plan.actor.company_id)
            .bind(&final_source.source_hash)
            .fetch_one(&mut *self.connection)
            .await?;
            if final_source.status != "pending"
                || final_source.version != 1
                || final_source.attempts != 0
                || final_source.recipient_email != plan.recipient_email
                || final_source.subject != plan.subject
                || final_source.body_text != plan.body
                || !authorization
            {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }

    fn commit_retry<'a>(
        &'a mut self,
        plan: &'a RetryPlan<'a>,
        now: i64,
    ) -> NotificationFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ready()?;
            let changed = sqlx::query_scalar::<_, String>(
                "UPDATE commerce_customer_notifications SET status='pending',version=version+1,\
                 attempts=0,next_attempt_at=$1,lease_until=NULL,last_error=NULL,updated_at=$1 \
                 WHERE id=$2 AND company_id=$3 AND version=$4 AND status='failed' RETURNING id",
            )
            .bind(now)
            .bind(plan.source.id.as_str())
            .bind(&plan.actor.company_id)
            .bind(plan.input.expected_version)
            .fetch_optional(&mut *self.connection)
            .await?;
            if changed.is_none() {
                return Err(version_conflict().into());
            }
            let result = plan.receipt.wire().to_string();
            self.ledger(
                plan.actor,
                plan.key,
                crate::domain::notifications::RETRY_OPERATION,
                plan.request_hash,
                &plan.source.id,
                &result,
                plan.receipt.version,
                "Customer notification retried",
                now,
            )
            .await?;
            self.flush().await?;
            if !self.authority(plan.actor, "commerce.sales", now).await? {
                return Err(access_changed().into());
            }
            self.capability(
                &plan.actor.company_id,
                NotificationPurpose::SettleExisting,
                now,
            )
            .await?;
            let final_source = self
                .load_notice(&plan.source.id)
                .await?
                .ok_or_else(readback_invalid)?;
            if final_source.status != "pending"
                || final_source.version != plan.receipt.version
                || final_source.attempts != 0
            {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }
}
