use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_application::{
    notifications::{
        DeliveryAttempt, DeliveryState, InboxState, Notification, NotificationChannel,
        NotificationError, NotificationRepositoryPort,
    },
    signature_orchestration::{SignatureEnvelopeRepositoryPort, SignatureOrchestrationError},
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, ItemDescriptorDraft,
    record_policy_id,
};
use erpx_kernel::{DataClassification, TenantId, TenantScope};
use erpx_legal::{
    LegalDocumentId, SignatureEnvelope, SignatureEnvelopeId, SignatureOrder, SignatureRecipient,
    SignatureStatus,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const NOTIFICATION_CONTEXT: &str = "shared.notifications";
const NOTIFICATION_AGGREGATE: &str = "notification";
const SIGNATURE_CONTEXT: &str = "shared.signatures";
const SIGNATURE_AGGREGATE: &str = "signature_envelope";
const NOTIFICATION_PROJECTION: &str = "shared.notifications";

#[derive(Debug, Clone, PartialEq)]
pub enum SharedProjectionRead {
    Active {
        generation: Uuid,
        value: Option<serde_json::Value>,
    },
    Degraded,
}

#[derive(Debug, Clone)]
pub struct SqlxSharedEnterpriseServiceStore {
    write: PgPool,
    read: Option<PgPool>,
    tenant_id: TenantId,
}

impl SqlxSharedEnterpriseServiceStore {
    /// Creates an immutable tenant-bound write capability. Every transaction
    /// verifies this tenant before touching shared or CQRS authority tables.
    pub const fn new(write: PgPool, tenant_id: TenantId) -> Self {
        Self {
            write,
            read: None,
            tenant_id,
        }
    }

    /// Adds the tenant read plane without changing the bound tenant authority.
    pub const fn with_read_pool(write: PgPool, read: PgPool, tenant_id: TenantId) -> Self {
        Self {
            write,
            read: Some(read),
            tenant_id,
        }
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
        let mut transaction = self.write.begin().await?;
        set_tenant(&mut transaction, self.tenant_id).await?;
        Ok(transaction)
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
        let pool = self.read.as_ref().ok_or_else(|| {
            sqlx::Error::Protocol("shared read-plane capability is not configured".into())
        })?;
        let mut transaction = pool.begin().await?;
        set_tenant(&mut transaction, self.tenant_id).await?;
        Ok(transaction)
    }

    /// Reads only the exact active notification generation. A missing active
    /// generation is explicitly degraded; building/candidate rows are never
    /// returned as a compatibility fallback.
    pub async fn read_notification_projection(
        &self,
        partition_key: &str,
        notification_id: Uuid,
    ) -> Result<SharedProjectionRead, sqlx::Error> {
        let mut transaction = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT active.generation, projection.state
             FROM erpx_cqrs_read.projection_generations active
             LEFT JOIN erpx_shared_read.notification_projection projection
               ON projection.tenant_id = active.tenant_id
              AND projection.projection_name = active.projection_name
              AND projection.generation = active.generation
              AND projection.partition_key = $3
              AND projection.notification_id = $4
             WHERE active.tenant_id = $1
               AND active.projection_name = $2
               AND active.status = 'active'",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(NOTIFICATION_PROJECTION)
        .bind(partition_key)
        .bind(notification_id)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(match row {
            Some(row) => SharedProjectionRead::Active {
                generation: row.try_get("generation")?,
                value: row.try_get("state")?,
            },
            None => SharedProjectionRead::Degraded,
        })
    }

    async fn load_notification(
        &self,
        notification_id: Uuid,
    ) -> Result<Option<Notification>, sqlx::Error> {
        let mut transaction = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT notification_id, tenant_scope, source_id, source_record_ref,
                    recipient_ref, classification, preview, inbox_state,
                    mandatory_policy_version, aggregate_version, delivery_version,
                    created_at, updated_at
             FROM erpx_shared_write.notifications
             WHERE tenant_id = $1 AND notification_id = $2",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(notification_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };

        let attempt_rows = sqlx::query(
            "SELECT recipient_ref, channel, delivery_version, dedupe_key,
                    delivery_status, provider_message_ref, last_provider_sequence,
                    attempt_count, last_error_code
             FROM erpx_shared_write.notification_delivery_attempts
             WHERE tenant_id = $1 AND notification_id = $2
             ORDER BY delivery_version, channel, recipient_ref",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(notification_id)
        .fetch_all(&mut *transaction)
        .await?;
        let mut attempts = Vec::with_capacity(attempt_rows.len());
        for attempt in attempt_rows {
            let channel: String = attempt.try_get("channel")?;
            let delivery_version = positive_u64(attempt.try_get("delivery_version")?)?;
            let recipient_ref: String = attempt.try_get("recipient_ref")?;
            let callback_ids = sqlx::query_scalar::<_, String>(
                "SELECT callback_id
                 FROM erpx_shared_write.notification_delivery_callbacks
                 WHERE tenant_id=$1 AND notification_id=$2 AND recipient_ref=$3
                   AND channel=$4 AND delivery_version=$5
                 ORDER BY callback_ordinal",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(notification_id)
            .bind(&recipient_ref)
            .bind(&channel)
            .bind(
                i64::try_from(delivery_version)
                    .map_err(|_| protocol("notification delivery version is outside i64"))?,
            )
            .fetch_all(&mut *transaction)
            .await?;
            attempts.push(DeliveryAttempt {
                channel: parse_notification_channel(&channel)?,
                delivery_version,
                dedupe_key: attempt.try_get("dedupe_key")?,
                state: parse_delivery_state(&attempt.try_get::<String, _>("delivery_status")?)?,
                provider_message_ref: attempt.try_get("provider_message_ref")?,
                last_provider_sequence: nonnegative_u64(
                    attempt.try_get("last_provider_sequence")?,
                )?,
                processed_callback_ids: callback_ids,
                attempt_count: u16::try_from(attempt.try_get::<i32, _>("attempt_count")?)
                    .map_err(|_| protocol("notification attempt count is outside u16"))?,
                last_error_code: attempt.try_get("last_error_code")?,
            });
        }

        let notification = Notification {
            notification_id: row.try_get("notification_id")?,
            version: positive_u64(row.try_get("aggregate_version")?)?,
            delivery_version: positive_u64(row.try_get("delivery_version")?)?,
            scope: serde_json::from_value(row.try_get("tenant_scope")?)
                .map_err(|error| protocol(error.to_string()))?,
            source_id: row.try_get("source_id")?,
            source_record_ref: row.try_get("source_record_ref")?,
            recipient_ref: row.try_get("recipient_ref")?,
            classification: parse_classification(&row.try_get::<String, _>("classification")?)?,
            preview: row.try_get("preview")?,
            inbox_state: parse_inbox_state(&row.try_get::<String, _>("inbox_state")?)?,
            mandatory_policy_version: row.try_get("mandatory_policy_version")?,
            attempts,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        };
        transaction.commit().await?;
        Ok(Some(notification))
    }

    async fn persist_notification(
        &self,
        notification: &Notification,
        expected_version: u64,
    ) -> Result<MutationDisposition, PersistError> {
        let next_version = exact_next_version(expected_version, notification.version)?;
        let serialized = serde_json::to_vec(notification).map_err(|_| PersistError::Invalid)?;
        let request_hash = Sha256::digest(&serialized).to_vec();
        let evidence_sha256 = digest_bytes(&serialized);
        let partition_key = scope_key(&notification.scope);
        let identity = MutationIdentity::new(
            NOTIFICATION_CONTEXT,
            NOTIFICATION_AGGREGATE,
            notification.notification_id,
            next_version,
        );
        let mut transaction = self.begin_write().await?;
        match reserve_command(
            &mut transaction,
            self.tenant_id,
            &partition_key,
            &identity,
            &request_hash,
        )
        .await?
        {
            MutationDisposition::Replayed => {
                transaction.commit().await?;
                return Ok(MutationDisposition::Replayed);
            }
            MutationDisposition::Applied => {}
        }

        let scope = serde_json::to_value(&notification.scope).map_err(|_| PersistError::Invalid)?;
        let changed = if expected_version == 0 {
            sqlx::query(
                "INSERT INTO erpx_shared_write.notifications
                    (tenant_id,notification_id,scope_key,tenant_scope,source_id,
                     source_record_ref,recipient_ref,classification,preview,inbox_state,
                     mandatory_policy_version,aggregate_version,delivery_version,
                     evidence_sha256,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
                 ON CONFLICT (tenant_id, notification_id) DO NOTHING",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(notification.notification_id)
            .bind(&partition_key)
            .bind(&scope)
            .bind(&notification.source_id)
            .bind(&notification.source_record_ref)
            .bind(&notification.recipient_ref)
            .bind(classification_name(notification.classification))
            .bind(&notification.preview)
            .bind(inbox_state_name(notification.inbox_state))
            .bind(&notification.mandatory_policy_version)
            .bind(i64_from_u64(notification.version)?)
            .bind(i64_from_u64(notification.delivery_version)?)
            .bind(&evidence_sha256)
            .bind(notification.created_at)
            .bind(notification.updated_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "UPDATE erpx_shared_write.notifications
                 SET preview=$9, inbox_state=$10,
                     mandatory_policy_version=$11, aggregate_version=$12,
                     delivery_version=$13, evidence_sha256=$14, updated_at=$15
                 WHERE tenant_id=$1 AND notification_id=$2 AND scope_key=$3
                   AND tenant_scope=$4 AND source_id=$5 AND source_record_ref=$6
                   AND recipient_ref=$7 AND classification=$8 AND created_at=$16
                   AND aggregate_version=$17",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(notification.notification_id)
            .bind(&partition_key)
            .bind(&scope)
            .bind(&notification.source_id)
            .bind(&notification.source_record_ref)
            .bind(&notification.recipient_ref)
            .bind(classification_name(notification.classification))
            .bind(&notification.preview)
            .bind(inbox_state_name(notification.inbox_state))
            .bind(&notification.mandatory_policy_version)
            .bind(i64_from_u64(notification.version)?)
            .bind(i64_from_u64(notification.delivery_version)?)
            .bind(&evidence_sha256)
            .bind(notification.updated_at)
            .bind(notification.created_at)
            .bind(i64_from_u64(expected_version)?)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        };
        if changed != 1 {
            return Err(PersistError::Conflict);
        }

        for attempt in &notification.attempts {
            let attempt_changed = sqlx::query(
                "INSERT INTO erpx_shared_write.notification_delivery_attempts
                    (tenant_id,notification_id,recipient_ref,channel,delivery_version,
                     dedupe_key,delivery_status,provider_message_ref,last_provider_sequence,
                     attempt_count,last_error_code,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                 ON CONFLICT (tenant_id,notification_id,recipient_ref,channel,delivery_version)
                 DO UPDATE SET delivery_status=EXCLUDED.delivery_status,
                    provider_message_ref=EXCLUDED.provider_message_ref,
                    last_provider_sequence=EXCLUDED.last_provider_sequence,
                    attempt_count=EXCLUDED.attempt_count,
                    last_error_code=EXCLUDED.last_error_code,
                    updated_at=EXCLUDED.updated_at
                 WHERE erpx_shared_write.notification_delivery_attempts.dedupe_key
                       = EXCLUDED.dedupe_key",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(notification.notification_id)
            .bind(&notification.recipient_ref)
            .bind(notification_channel_name(attempt.channel))
            .bind(i64_from_u64(attempt.delivery_version)?)
            .bind(&attempt.dedupe_key)
            .bind(delivery_state_name(attempt.state))
            .bind(&attempt.provider_message_ref)
            .bind(i64_from_u64(attempt.last_provider_sequence)?)
            .bind(i32::from(attempt.attempt_count))
            .bind(&attempt.last_error_code)
            .bind(notification.updated_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if attempt_changed != 1 {
                return Err(PersistError::Conflict);
            }
            for (index, callback_id) in attempt.processed_callback_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO erpx_shared_write.notification_delivery_callbacks
                        (tenant_id,notification_id,recipient_ref,channel,delivery_version,
                         callback_id,callback_ordinal,processed_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                     ON CONFLICT (tenant_id,notification_id,recipient_ref,channel,
                                  delivery_version,callback_id) DO NOTHING",
                )
                .bind(self.tenant_id.as_uuid())
                .bind(notification.notification_id)
                .bind(&notification.recipient_ref)
                .bind(notification_channel_name(attempt.channel))
                .bind(i64_from_u64(attempt.delivery_version)?)
                .bind(callback_id)
                .bind(positive_i32(index + 1)?)
                .bind(notification.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            let callback_count: i64 = sqlx::query_scalar(
                "SELECT count(*)
                 FROM erpx_shared_write.notification_delivery_callbacks
                 WHERE tenant_id=$1 AND notification_id=$2 AND recipient_ref=$3
                   AND channel=$4 AND delivery_version=$5",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(notification.notification_id)
            .bind(&notification.recipient_ref)
            .bind(notification_channel_name(attempt.channel))
            .bind(i64_from_u64(attempt.delivery_version)?)
            .fetch_one(&mut *transaction)
            .await?;
            if usize::try_from(callback_count).ok() != Some(attempt.processed_callback_ids.len()) {
                return Err(PersistError::Conflict);
            }
        }
        let attempt_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_shared_write.notification_delivery_attempts
             WHERE tenant_id=$1 AND notification_id=$2",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(notification.notification_id)
        .fetch_one(&mut *transaction)
        .await?;
        if usize::try_from(attempt_count).ok() != Some(notification.attempts.len()) {
            return Err(PersistError::Conflict);
        }
        finalize_command(
            &mut transaction,
            self.tenant_id,
            &partition_key,
            &scope,
            &identity,
            &evidence_sha256,
            inbox_state_name(notification.inbox_state),
            notification.updated_at,
        )
        .await?;
        transaction.commit().await?;
        Ok(MutationDisposition::Applied)
    }

    async fn load_signature_by(
        &self,
        envelope_id: Option<Uuid>,
        provider_envelope_ref: Option<&str>,
    ) -> Result<Option<SignatureEnvelope>, sqlx::Error> {
        let mut transaction = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT envelope_id, tenant_scope, document_id, document_version,
                    document_sha256, provider_key, provider_policy_version,
                    signature_order, retention_until, envelope_status,
                    provider_envelope_ref, last_provider_sequence, evidence_sha256,
                    created_by, cancelled_by, aggregate_version, created_at, updated_at
             FROM erpx_shared_write.signature_envelopes
             WHERE tenant_id=$1
               AND ($2::uuid IS NULL OR envelope_id=$2)
               AND ($3::text IS NULL OR provider_envelope_ref=$3)",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(envelope_id)
        .bind(provider_envelope_ref)
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(row) = row else {
            transaction.commit().await?;
            return Ok(None);
        };
        let envelope_id: Uuid = row.try_get("envelope_id")?;
        let recipient_rows = sqlx::query(
            "SELECT recipient_id, recipient_order, subject_realm, principal_ref,
                    external_subject_ref, access_grant_sha256, access_expires_at, signed_at
             FROM erpx_shared_write.signature_recipients
             WHERE tenant_id=$1 AND envelope_id=$2
             ORDER BY recipient_order, recipient_id",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(envelope_id)
        .fetch_all(&mut *transaction)
        .await?;
        let mut recipients = Vec::with_capacity(recipient_rows.len());
        for recipient in recipient_rows {
            let subject_realm: String = recipient.try_get("subject_realm")?;
            let subject = match subject_realm.as_str() {
                "internal" => serde_json::json!({
                    "realm": "internal",
                    "principal_ref": recipient.try_get::<String, _>("principal_ref")?,
                }),
                "external_one_time" => serde_json::json!({
                    "realm": "external_one_time",
                    "subject_ref": recipient.try_get::<String, _>("external_subject_ref")?,
                    "access_grant_sha256": recipient.try_get::<String, _>("access_grant_sha256")?,
                    "expires_at": recipient.try_get::<DateTime<Utc>, _>("access_expires_at")?,
                }),
                _ => return Err(protocol("unknown signature subject realm")),
            };
            recipients.push(serde_json::json!({
                "id": recipient.try_get::<Uuid, _>("recipient_id")?,
                "subject": subject,
                "order": u16::try_from(recipient.try_get::<i32, _>("recipient_order")?)
                    .map_err(|_| protocol("signature recipient order is outside u16"))?,
                "signed_at": recipient.try_get::<Option<DateTime<Utc>>, _>("signed_at")?,
            }));
        }
        let processed_provider_events = sqlx::query_scalar::<_, String>(
            "SELECT provider_event_id
             FROM erpx_shared_write.signature_provider_events
             WHERE tenant_id=$1 AND envelope_id=$2
             ORDER BY event_ordinal",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(envelope_id)
        .fetch_all(&mut *transaction)
        .await?;
        let value = serde_json::json!({
            "id": envelope_id,
            "version": positive_u64(row.try_get("aggregate_version")?)?,
            "scope": row.try_get::<serde_json::Value, _>("tenant_scope")?,
            "document_id": row.try_get::<Uuid, _>("document_id")?,
            "document_version": positive_u64(row.try_get("document_version")?)?,
            "document_sha256": row.try_get::<String, _>("document_sha256")?,
            "provider_key": row.try_get::<String, _>("provider_key")?,
            "provider_policy_version": row.try_get::<String, _>("provider_policy_version")?,
            "order": row.try_get::<String, _>("signature_order")?,
            "recipients": recipients,
            "retention_until": row.try_get::<DateTime<Utc>, _>("retention_until")?,
            "status": row.try_get::<String, _>("envelope_status")?,
            "provider_envelope_ref": row.try_get::<Option<String>, _>("provider_envelope_ref")?,
            "last_provider_sequence": nonnegative_u64(row.try_get("last_provider_sequence")?)?,
            "processed_provider_events": processed_provider_events,
            "evidence_sha256": row.try_get::<Option<String>, _>("evidence_sha256")?,
            "created_by": row.try_get::<String, _>("created_by")?,
            "cancelled_by": row.try_get::<Option<String>, _>("cancelled_by")?,
            "created_at": row.try_get::<DateTime<Utc>, _>("created_at")?,
            "updated_at": row.try_get::<DateTime<Utc>, _>("updated_at")?,
        });
        let envelope =
            serde_json::from_value(value).map_err(|error| protocol(error.to_string()))?;
        transaction.commit().await?;
        Ok(Some(envelope))
    }

    async fn persist_signature(
        &self,
        envelope: &SignatureEnvelope,
        expected_version: u64,
        mutation_evidence_sha256: &str,
    ) -> Result<MutationDisposition, PersistError> {
        if !valid_digest(mutation_evidence_sha256) || envelope.validate().is_err() {
            return Err(PersistError::Invalid);
        }
        let record: SignatureEnvelopeRecord = serde_json::from_value(
            serde_json::to_value(envelope).map_err(|_| PersistError::Invalid)?,
        )
        .map_err(|_| PersistError::Invalid)?;
        let next_version = exact_next_version(expected_version, record.version)?;
        let serialized = serde_json::to_vec(&record).map_err(|_| PersistError::Invalid)?;
        let request_hash = Sha256::digest(&serialized).to_vec();
        let partition_key = scope_key(&record.scope);
        let identity = MutationIdentity::new(
            SIGNATURE_CONTEXT,
            SIGNATURE_AGGREGATE,
            record.id.as_uuid(),
            next_version,
        );
        let mut transaction = self.begin_write().await?;
        match reserve_command(
            &mut transaction,
            self.tenant_id,
            &partition_key,
            &identity,
            &request_hash,
        )
        .await?
        {
            MutationDisposition::Replayed => {
                transaction.commit().await?;
                return Ok(MutationDisposition::Replayed);
            }
            MutationDisposition::Applied => {}
        }
        let scope = serde_json::to_value(&record.scope).map_err(|_| PersistError::Invalid)?;
        let changed = if expected_version == 0 {
            sqlx::query(
                "INSERT INTO erpx_shared_write.signature_envelopes
                    (tenant_id,envelope_id,scope_key,tenant_scope,document_id,document_version,
                     document_sha256,provider_key,provider_policy_version,signature_order,
                     retention_until,envelope_status,provider_envelope_ref,last_provider_sequence,
                     evidence_sha256,mutation_evidence_sha256,created_by,cancelled_by,
                     aggregate_version,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
                 ON CONFLICT (tenant_id, envelope_id) DO NOTHING",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(record.id.as_uuid())
            .bind(&partition_key)
            .bind(&scope)
            .bind(record.document_id.as_uuid())
            .bind(i64_from_u64(record.document_version)?)
            .bind(&record.document_sha256)
            .bind(&record.provider_key)
            .bind(&record.provider_policy_version)
            .bind(signature_order_name(record.order))
            .bind(record.retention_until)
            .bind(signature_status_name(record.status))
            .bind(&record.provider_envelope_ref)
            .bind(i64_from_u64(record.last_provider_sequence)?)
            .bind(&record.evidence_sha256)
            .bind(mutation_evidence_sha256)
            .bind(&record.created_by)
            .bind(&record.cancelled_by)
            .bind(i64_from_u64(record.version)?)
            .bind(record.created_at)
            .bind(record.updated_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "UPDATE erpx_shared_write.signature_envelopes
                 SET provider_envelope_ref=$5, last_provider_sequence=$6,
                     evidence_sha256=$7, mutation_evidence_sha256=$8,
                     cancelled_by=$9, envelope_status=$10, aggregate_version=$11,
                     updated_at=$12
                 WHERE tenant_id=$1 AND envelope_id=$2 AND scope_key=$3
                   AND tenant_scope=$4 AND aggregate_version=$13
                   AND document_id=$14 AND document_version=$15
                   AND document_sha256=$16 AND provider_key=$17
                   AND provider_policy_version=$18 AND signature_order=$19
                   AND retention_until=$20 AND created_by=$21 AND created_at=$22",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(record.id.as_uuid())
            .bind(&partition_key)
            .bind(&scope)
            .bind(&record.provider_envelope_ref)
            .bind(i64_from_u64(record.last_provider_sequence)?)
            .bind(&record.evidence_sha256)
            .bind(mutation_evidence_sha256)
            .bind(&record.cancelled_by)
            .bind(signature_status_name(record.status))
            .bind(i64_from_u64(record.version)?)
            .bind(record.updated_at)
            .bind(i64_from_u64(expected_version)?)
            .bind(record.document_id.as_uuid())
            .bind(i64_from_u64(record.document_version)?)
            .bind(&record.document_sha256)
            .bind(&record.provider_key)
            .bind(&record.provider_policy_version)
            .bind(signature_order_name(record.order))
            .bind(record.retention_until)
            .bind(&record.created_by)
            .bind(record.created_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        };
        if changed != 1 {
            return Err(PersistError::Conflict);
        }

        for recipient in &record.recipients {
            let (realm, principal, external, digest, expires) = match &recipient.subject {
                erpx_legal::SignatureSubject::Internal { principal_ref } => {
                    ("internal", Some(principal_ref.as_str()), None, None, None)
                }
                erpx_legal::SignatureSubject::ExternalOneTime {
                    subject_ref,
                    access_grant_sha256,
                    expires_at,
                } => (
                    "external_one_time",
                    None,
                    Some(subject_ref.as_str()),
                    Some(access_grant_sha256.as_str()),
                    Some(*expires_at),
                ),
            };
            let recipient_changed = sqlx::query(
                "INSERT INTO erpx_shared_write.signature_recipients
                    (tenant_id,envelope_id,recipient_id,recipient_order,subject_realm,
                     principal_ref,external_subject_ref,access_grant_sha256,
                     access_expires_at,signed_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                 ON CONFLICT (tenant_id,envelope_id,recipient_id)
                 DO UPDATE SET signed_at=EXCLUDED.signed_at
                 WHERE erpx_shared_write.signature_recipients.recipient_order
                           = EXCLUDED.recipient_order
                   AND erpx_shared_write.signature_recipients.subject_realm
                           = EXCLUDED.subject_realm
                   AND erpx_shared_write.signature_recipients.principal_ref
                           IS NOT DISTINCT FROM EXCLUDED.principal_ref
                   AND erpx_shared_write.signature_recipients.external_subject_ref
                           IS NOT DISTINCT FROM EXCLUDED.external_subject_ref
                   AND erpx_shared_write.signature_recipients.access_grant_sha256
                           IS NOT DISTINCT FROM EXCLUDED.access_grant_sha256
                   AND erpx_shared_write.signature_recipients.access_expires_at
                           IS NOT DISTINCT FROM EXCLUDED.access_expires_at
                   AND (erpx_shared_write.signature_recipients.signed_at IS NULL
                        OR erpx_shared_write.signature_recipients.signed_at
                           IS NOT DISTINCT FROM EXCLUDED.signed_at)",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(record.id.as_uuid())
            .bind(recipient.id.as_uuid())
            .bind(i32::from(recipient.order))
            .bind(realm)
            .bind(principal)
            .bind(external)
            .bind(digest)
            .bind(expires)
            .bind(recipient.signed_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if recipient_changed != 1 {
                return Err(PersistError::Conflict);
            }
        }
        let recipient_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_shared_write.signature_recipients
             WHERE tenant_id=$1 AND envelope_id=$2",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(record.id.as_uuid())
        .fetch_one(&mut *transaction)
        .await?;
        if usize::try_from(recipient_count).ok() != Some(record.recipients.len()) {
            return Err(PersistError::Conflict);
        }

        let stored_events = sqlx::query_scalar::<_, String>(
            "SELECT provider_event_id
             FROM erpx_shared_write.signature_provider_events
             WHERE tenant_id=$1 AND envelope_id=$2 ORDER BY event_ordinal",
        )
        .bind(self.tenant_id.as_uuid())
        .bind(record.id.as_uuid())
        .fetch_all(&mut *transaction)
        .await?;
        if record.processed_provider_events.len() < stored_events.len()
            || record.processed_provider_events[..stored_events.len()] != stored_events
        {
            return Err(PersistError::Conflict);
        }
        for (index, event_id) in record.processed_provider_events.iter().enumerate() {
            sqlx::query(
                "INSERT INTO erpx_shared_write.signature_provider_events
                    (tenant_id,envelope_id,provider_event_id,event_ordinal,processed_at)
                 VALUES ($1,$2,$3,$4,$5)
                 ON CONFLICT (tenant_id,envelope_id,provider_event_id) DO NOTHING",
            )
            .bind(self.tenant_id.as_uuid())
            .bind(record.id.as_uuid())
            .bind(event_id)
            .bind(positive_i32(index + 1)?)
            .bind(record.updated_at)
            .execute(&mut *transaction)
            .await?;
        }
        finalize_command(
            &mut transaction,
            self.tenant_id,
            &partition_key,
            &scope,
            &identity,
            mutation_evidence_sha256,
            signature_status_name(record.status),
            record.updated_at,
        )
        .await?;
        append_signature_owner_facts(
            &mut transaction,
            self.tenant_id,
            &partition_key,
            &scope,
            &identity,
            envelope,
            record.status,
            record.retention_until,
            record.updated_at,
        )
        .await?;
        transaction.commit().await?;
        Ok(MutationDisposition::Applied)
    }
}

#[derive(Debug, Serialize)]
struct SignatureProjectionFact<'a> {
    envelope_id: Uuid,
    envelope_version: u64,
    tenant_scope: &'a TenantScope,
    document_id: Uuid,
    document_version: u64,
    envelope_status: &'static str,
    retention_until: DateTime<Utc>,
}

#[allow(clippy::too_many_arguments)]
async fn append_signature_owner_facts(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
    partition_key: &str,
    tenant_scope: &serde_json::Value,
    identity: &MutationIdentity,
    envelope: &SignatureEnvelope,
    status: SignatureStatus,
    retention_until: DateTime<Utc>,
    occurred_at: DateTime<Utc>,
) -> Result<(), PersistError> {
    let record = envelope.record_ref().map_err(|_| PersistError::Invalid)?;
    let descriptor = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PersistError::Invalid)?,
        record,
        scope: envelope.scope().clone(),
        lifecycle_state: signature_status_name(status).into(),
        classification: DataClassification::Restricted,
        descriptor_version: envelope.version(),
        authorized_search_text: format!(
            "signature envelope {} document {}",
            envelope.id().as_uuid(),
            envelope.document_id().as_uuid()
        ),
    };
    descriptor.validate().map_err(|_| PersistError::Invalid)?;
    let actor = serde_json::json!({"principal_type":"service","principal_ref":"shared-enterprise-service-store"});
    let projection = SignatureProjectionFact {
        envelope_id: envelope.id().as_uuid(),
        envelope_version: envelope.version(),
        tenant_scope: envelope.scope(),
        document_id: envelope.document_id().as_uuid(),
        document_version: envelope.document_version(),
        envelope_status: signature_status_name(status),
        retention_until,
    };
    for (event_id, event_type, payload) in [
        (
            deterministic_uuid("signature-projection-event", &identity.idempotency_key),
            "legal.signature_envelope_changed",
            serde_json::to_value(projection).map_err(|_| PersistError::Invalid)?,
        ),
        (
            deterministic_uuid("signature-directory-event", &identity.idempotency_key),
            ITEM_DIRECTORY_OWNER_EVENT_TYPE,
            serde_json::to_value(descriptor).map_err(|_| PersistError::Invalid)?,
        ),
    ] {
        sqlx::query(
            "INSERT INTO erpx_cqrs_write.outbox_events
                (tenant_id,event_id,context_name,partition_key,command_id,event_type,event_version,
                 aggregate_type,aggregate_id,aggregate_version,tenant_scope,actor,correlation_id,
                 causation_id,payload,occurred_at,available_at)
             VALUES ($1,$2,'legal',$3,$4,$5,$6,'signature_envelope',$7,$8,$9,$10,$11,NULL,$12,$13,$13)",
        )
        .bind(tenant_id.as_uuid()).bind(event_id).bind(partition_key).bind(identity.command_id)
        .bind(event_type).bind(i32::try_from(ITEM_DIRECTORY_FACT_VERSION).map_err(|_| PersistError::Invalid)?)
        .bind(identity.aggregate_id).bind(i64_from_u64(identity.aggregate_version)?)
        .bind(tenant_scope).bind(&actor).bind(identity.correlation_id).bind(payload).bind(occurred_at)
        .execute(&mut **transaction).await?;
    }
    Ok(())
}

#[async_trait]
impl NotificationRepositoryPort for SqlxSharedEnterpriseServiceStore {
    async fn load(&self, notification_id: Uuid) -> Result<Option<Notification>, NotificationError> {
        if notification_id.is_nil() {
            return Err(NotificationError::Invalid);
        }
        self.load_notification(notification_id)
            .await
            .map_err(|_| NotificationError::ProviderUnavailable)
    }

    async fn save(
        &self,
        notification: &Notification,
        expected_version: u64,
    ) -> Result<(), NotificationError> {
        match self
            .persist_notification(notification, expected_version)
            .await
        {
            Ok(MutationDisposition::Applied | MutationDisposition::Replayed) => Ok(()),
            Err(PersistError::Conflict) => Err(NotificationError::Conflict),
            Err(PersistError::Invalid) => Err(NotificationError::Invalid),
            Err(PersistError::Database(error)) if database_conflict(&error) => {
                Err(NotificationError::Conflict)
            }
            Err(PersistError::Database(_)) => Err(NotificationError::ProviderUnavailable),
        }
    }
}

#[async_trait]
impl SignatureEnvelopeRepositoryPort for SqlxSharedEnterpriseServiceStore {
    async fn load(
        &self,
        envelope_id: SignatureEnvelopeId,
    ) -> Result<Option<SignatureEnvelope>, SignatureOrchestrationError> {
        self.load_signature_by(Some(envelope_id.as_uuid()), None)
            .await
            .map_err(|_| SignatureOrchestrationError::Unavailable)
    }

    async fn load_by_provider_ref(
        &self,
        provider_envelope_ref: &str,
    ) -> Result<Option<SignatureEnvelope>, SignatureOrchestrationError> {
        if provider_envelope_ref.trim().is_empty() || provider_envelope_ref.len() > 255 {
            return Err(SignatureOrchestrationError::Denied);
        }
        self.load_signature_by(None, Some(provider_envelope_ref))
            .await
            .map_err(|_| SignatureOrchestrationError::Unavailable)
    }

    async fn save(
        &self,
        envelope: &SignatureEnvelope,
        expected_version: u64,
        evidence_sha256: &str,
    ) -> Result<(), SignatureOrchestrationError> {
        match self
            .persist_signature(envelope, expected_version, evidence_sha256)
            .await
        {
            Ok(MutationDisposition::Applied | MutationDisposition::Replayed) => Ok(()),
            Err(PersistError::Conflict) => Err(SignatureOrchestrationError::Conflict),
            Err(PersistError::Invalid) => Err(SignatureOrchestrationError::Invalid),
            Err(PersistError::Database(error)) if database_conflict(&error) => {
                Err(SignatureOrchestrationError::Conflict)
            }
            Err(PersistError::Database(_)) => Err(SignatureOrchestrationError::Unavailable),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MutationDisposition {
    Applied,
    Replayed,
}

#[derive(Debug)]
enum PersistError {
    Database(sqlx::Error),
    Conflict,
    Invalid,
}

impl From<sqlx::Error> for PersistError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

#[derive(Debug)]
struct MutationIdentity {
    context_name: &'static str,
    aggregate_type: &'static str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    idempotency_key: String,
    command_id: Uuid,
    correlation_id: Uuid,
    audit_id: Uuid,
    event_id: Uuid,
}

impl MutationIdentity {
    fn new(
        context_name: &'static str,
        aggregate_type: &'static str,
        aggregate_id: Uuid,
        aggregate_version: u64,
    ) -> Self {
        let idempotency_key = format!("{aggregate_type}:{aggregate_id}:v{aggregate_version}");
        Self {
            context_name,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            command_id: deterministic_uuid("command", &idempotency_key),
            correlation_id: deterministic_uuid("correlation", &idempotency_key),
            audit_id: deterministic_uuid("audit", &idempotency_key),
            event_id: deterministic_uuid("event", &idempotency_key),
            idempotency_key,
        }
    }
}

async fn reserve_command(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
    partition_key: &str,
    identity: &MutationIdentity,
    request_hash: &[u8],
) -> Result<MutationDisposition, PersistError> {
    let inserted = sqlx::query(
        "INSERT INTO erpx_cqrs_write.command_receipts
            (tenant_id,context_name,partition_key,idempotency_key,command_id,
             request_hash,correlation_id,status,created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,'reserved',clock_timestamp())
         ON CONFLICT (tenant_id,context_name,partition_key,idempotency_key) DO NOTHING",
    )
    .bind(tenant_id.as_uuid())
    .bind(identity.context_name)
    .bind(partition_key)
    .bind(&identity.idempotency_key)
    .bind(identity.command_id)
    .bind(request_hash)
    .bind(identity.correlation_id)
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    if inserted == 1 {
        return Ok(MutationDisposition::Applied);
    }
    let row = sqlx::query(
        "SELECT request_hash,status
         FROM erpx_cqrs_write.command_receipts
         WHERE tenant_id=$1 AND context_name=$2 AND partition_key=$3 AND idempotency_key=$4",
    )
    .bind(tenant_id.as_uuid())
    .bind(identity.context_name)
    .bind(partition_key)
    .bind(&identity.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(PersistError::Conflict)?;
    classify_receipt(
        &row.try_get::<Vec<u8>, _>("request_hash")?,
        &row.try_get::<String, _>("status")?,
        request_hash,
    )
}

fn classify_receipt(
    stored_hash: &[u8],
    stored_status: &str,
    request_hash: &[u8],
) -> Result<MutationDisposition, PersistError> {
    if stored_hash != request_hash {
        return Err(PersistError::Conflict);
    }
    match stored_status {
        "completed" => Ok(MutationDisposition::Replayed),
        // A reservation is committed only with its authority/audit/outbox work.
        // Seeing one here is corrupt/incomplete evidence and fails closed.
        _ => Err(PersistError::Conflict),
    }
}

#[allow(clippy::too_many_arguments)]
async fn finalize_command(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
    partition_key: &str,
    tenant_scope: &serde_json::Value,
    identity: &MutationIdentity,
    evidence_sha256: &str,
    lifecycle_state: &str,
    occurred_at: DateTime<Utc>,
) -> Result<(), PersistError> {
    let actor = serde_json::json!({
        "principal_type": "service",
        "principal_ref": "shared-enterprise-service-store"
    });
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.audit_log
            (tenant_id,audit_id,context_name,partition_key,command_id,correlation_id,
             tenant_scope,actor,audit_context,action,aggregate_type,aggregate_id,
             aggregate_version,details,occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'save',$10,$11,$12,$13,$14)",
    )
    .bind(tenant_id.as_uuid())
    .bind(identity.audit_id)
    .bind(identity.context_name)
    .bind(partition_key)
    .bind(identity.command_id)
    .bind(identity.correlation_id)
    .bind(tenant_scope)
    .bind(&actor)
    .bind(serde_json::json!({"authority": "shared-enterprise-service"}))
    .bind(identity.aggregate_type)
    .bind(identity.aggregate_id)
    .bind(i64_from_u64(identity.aggregate_version)?)
    .bind(serde_json::json!({"evidence_sha256": evidence_sha256}))
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?;

    // The outbox fact is intentionally opaque: no notification preview,
    // signatory subject, document content, or provider callback body.
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (tenant_id,event_id,context_name,partition_key,command_id,event_type,
             event_version,aggregate_type,aggregate_id,aggregate_version,tenant_scope,
             actor,correlation_id,causation_id,payload,occurred_at,available_at)
         VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8,$9,$10,$11,$12,NULL,$13,$14,$14)",
    )
    .bind(tenant_id.as_uuid())
    .bind(identity.event_id)
    .bind(identity.context_name)
    .bind(partition_key)
    .bind(identity.command_id)
    .bind(format!("{}.saved.v1", identity.aggregate_type))
    .bind(identity.aggregate_type)
    .bind(identity.aggregate_id)
    .bind(i64_from_u64(identity.aggregate_version)?)
    .bind(tenant_scope)
    .bind(&actor)
    .bind(identity.correlation_id)
    .bind(serde_json::json!({
        "record_id": identity.aggregate_id,
        "aggregate_version": identity.aggregate_version,
        "lifecycle_state": lifecycle_state,
        "evidence_sha256": evidence_sha256,
    }))
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?;

    let completed = sqlx::query(
        "UPDATE erpx_cqrs_write.command_receipts
         SET status='completed', receipt=$5, completed_at=$6
         WHERE tenant_id=$1 AND context_name=$2 AND partition_key=$3
           AND idempotency_key=$4 AND status='reserved'",
    )
    .bind(tenant_id.as_uuid())
    .bind(identity.context_name)
    .bind(partition_key)
    .bind(&identity.idempotency_key)
    .bind(serde_json::json!({
        "aggregate_id": identity.aggregate_id,
        "aggregate_version": identity.aggregate_version,
        "evidence_sha256": evidence_sha256,
    }))
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    if completed != 1 {
        return Err(PersistError::Conflict);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignatureEnvelopeRecord {
    id: SignatureEnvelopeId,
    version: u64,
    scope: TenantScope,
    document_id: LegalDocumentId,
    document_version: u64,
    document_sha256: String,
    provider_key: String,
    provider_policy_version: String,
    order: SignatureOrder,
    recipients: Vec<SignatureRecipient>,
    retention_until: DateTime<Utc>,
    status: SignatureStatus,
    provider_envelope_ref: Option<String>,
    last_provider_sequence: u64,
    processed_provider_events: Vec<String>,
    evidence_sha256: Option<String>,
    created_by: String,
    cancelled_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn set_tenant(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: TenantId,
) -> Result<(), sqlx::Error> {
    let expected = tenant_id.to_string();
    let bound: String = sqlx::query_scalar("SELECT set_config('app.tenant_id',$1,true)")
        .bind(&expected)
        .fetch_one(&mut **transaction)
        .await?;
    let verified: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.tenant_id',true),'')")
            .fetch_one(&mut **transaction)
            .await?;
    if bound != expected || verified.as_deref() != Some(expected.as_str()) {
        return Err(protocol("tenant RLS binding could not be verified"));
    }
    Ok(())
}

fn exact_next_version(expected: u64, actual: u64) -> Result<u64, PersistError> {
    let next = expected.checked_add(1).ok_or(PersistError::Invalid)?;
    if actual != next {
        return Err(PersistError::Conflict);
    }
    Ok(next)
}

fn scope_key(scope: &TenantScope) -> String {
    match scope {
        TenantScope::Platform => "platform".into(),
        TenantScope::Holding { holding_id } => format!("h:{holding_id}"),
        TenantScope::Company {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}"),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("h:{holding_id}/c:{company_id}/b:{branch_id}"),
        TenantScope::Subtree { .. } => serde_json::to_string(scope).expect("serializable scope"),
    }
}

fn deterministic_uuid(kind: &str, key: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"erpx.shared-enterprise.v2");
    hasher.update(kind.as_bytes());
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn digest_bytes(value: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value)))
}

fn valid_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn database_conflict(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|error| matches!(error.code().as_deref(), Some("23505" | "23503" | "23514")))
}

fn protocol(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Protocol(message.into())
}

fn i64_from_u64(value: u64) -> Result<i64, PersistError> {
    i64::try_from(value).map_err(|_| PersistError::Invalid)
}

fn positive_i32(value: usize) -> Result<i32, PersistError> {
    let value = i32::try_from(value).map_err(|_| PersistError::Invalid)?;
    (value > 0).then_some(value).ok_or(PersistError::Invalid)
}

fn positive_u64(value: i64) -> Result<u64, sqlx::Error> {
    if value <= 0 {
        return Err(protocol("persisted version must be positive"));
    }
    u64::try_from(value).map_err(|_| protocol("persisted version is outside u64"))
}

fn nonnegative_u64(value: i64) -> Result<u64, sqlx::Error> {
    if value < 0 {
        return Err(protocol("persisted sequence must be nonnegative"));
    }
    u64::try_from(value).map_err(|_| protocol("persisted sequence is outside u64"))
}

fn notification_channel_name(value: NotificationChannel) -> &'static str {
    match value {
        NotificationChannel::InApp => "in_app",
        NotificationChannel::Email => "email",
        NotificationChannel::Sms => "sms",
        NotificationChannel::Push => "push",
        NotificationChannel::Webhook => "webhook",
    }
}

fn parse_notification_channel(value: &str) -> Result<NotificationChannel, sqlx::Error> {
    match value {
        "in_app" => Ok(NotificationChannel::InApp),
        "email" => Ok(NotificationChannel::Email),
        "sms" => Ok(NotificationChannel::Sms),
        "push" => Ok(NotificationChannel::Push),
        "webhook" => Ok(NotificationChannel::Webhook),
        _ => Err(protocol("unknown notification channel")),
    }
}

fn inbox_state_name(value: InboxState) -> &'static str {
    match value {
        InboxState::Unread => "unread",
        InboxState::Read => "read",
        InboxState::Acknowledged => "acknowledged",
        InboxState::ActionRequired => "action_required",
        InboxState::Withdrawn => "withdrawn",
    }
}

fn parse_inbox_state(value: &str) -> Result<InboxState, sqlx::Error> {
    match value {
        "unread" => Ok(InboxState::Unread),
        "read" => Ok(InboxState::Read),
        "acknowledged" => Ok(InboxState::Acknowledged),
        "action_required" => Ok(InboxState::ActionRequired),
        "withdrawn" => Ok(InboxState::Withdrawn),
        _ => Err(protocol("unknown notification inbox state")),
    }
}

fn delivery_state_name(value: DeliveryState) -> &'static str {
    match value {
        DeliveryState::Queued => "queued",
        DeliveryState::Accepted => "accepted",
        DeliveryState::Delivered => "delivered",
        DeliveryState::Failed => "failed",
        DeliveryState::Uncertain => "uncertain",
        DeliveryState::Suppressed => "suppressed",
    }
}

fn parse_delivery_state(value: &str) -> Result<DeliveryState, sqlx::Error> {
    match value {
        "queued" => Ok(DeliveryState::Queued),
        "accepted" => Ok(DeliveryState::Accepted),
        "delivered" => Ok(DeliveryState::Delivered),
        "failed" => Ok(DeliveryState::Failed),
        "uncertain" => Ok(DeliveryState::Uncertain),
        "suppressed" => Ok(DeliveryState::Suppressed),
        _ => Err(protocol("unknown notification delivery state")),
    }
}

fn classification_name(value: DataClassification) -> &'static str {
    match value {
        DataClassification::Public => "public",
        DataClassification::Internal => "internal",
        DataClassification::Confidential => "confidential",
        DataClassification::Restricted => "restricted",
    }
}

fn parse_classification(value: &str) -> Result<DataClassification, sqlx::Error> {
    match value {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(protocol("unknown data classification")),
    }
}

fn signature_order_name(value: SignatureOrder) -> &'static str {
    match value {
        SignatureOrder::Parallel => "parallel",
        SignatureOrder::Serial => "serial",
    }
}

fn signature_status_name(value: SignatureStatus) -> &'static str {
    match value {
        SignatureStatus::Draft => "draft",
        SignatureStatus::Submitted => "submitted",
        SignatureStatus::InProgress => "in_progress",
        SignatureStatus::Completed => "completed",
        SignatureStatus::Failed => "failed",
        SignatureStatus::Cancelled => "cancelled",
        SignatureStatus::Expired => "expired",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_authority_is_normalized_tenant_composite_and_force_rls_complete() {
        let write = include_str!(
            "../../migration/migrations_tenant_write/0078_shared_enterprise_services.sql"
        );
        let read = include_str!(
            "../../migration/migrations_tenant_read/0069_shared_enterprise_services_projection.sql"
        );
        assert!(!write.contains("service_artifacts"));
        assert!(!write.contains("payload JSONB"));
        let normalized_write = write.split_whitespace().collect::<Vec<_>>().join(" ");
        for table in [
            "approval_sessions",
            "approval_decisions",
            "notifications",
            "notification_delivery_attempts",
            "notification_delivery_callbacks",
            "signature_envelopes",
            "signature_recipients",
            "signature_provider_events",
            "report_export_manifests",
            "import_row_receipts",
            "extension_schema_versions",
            "signature_callback_inbox",
        ] {
            assert!(write.contains(&format!("CREATE TABLE erpx_shared_write.{table}")));
            assert!(write.contains(&format!("'{table}'")));
            let definition = normalized_write
                .split(&format!("CREATE TABLE erpx_shared_write.{table}"))
                .nth(1)
                .expect("table definition")
                .split("CREATE TABLE")
                .next()
                .expect("bounded table definition");
            assert!(
                definition.contains("PRIMARY KEY (tenant_id"),
                "{table} primary authority is not tenant-composite"
            );
            for unique in definition.match_indices("UNIQUE (") {
                assert!(
                    definition[unique.0..].starts_with("UNIQUE (tenant_id"),
                    "{table} contains a non-tenant unique authority"
                );
            }
            for foreign_key in definition.match_indices("FOREIGN KEY (") {
                assert!(
                    definition[foreign_key.0..].starts_with("FOREIGN KEY (tenant_id"),
                    "{table} contains a cross-tenant foreign key"
                );
            }
        }
        assert!(write.contains("FOREIGN KEY (tenant_id, session_id)"));
        assert!(write.contains("FOREIGN KEY (tenant_id, notification_id)"));
        assert!(write.contains("FOREIGN KEY (tenant_id, envelope_id)"));
        assert!(write.contains("FORCE ROW LEVEL SECURITY"));
        assert!(read.contains("FOREIGN KEY (tenant_id, projection_name, generation)"));
        assert!(read.contains("FORCE ROW LEVEL SECURITY"));
        let normalized_read = read.split_whitespace().collect::<Vec<_>>().join(" ");
        for table in [
            "work_projection",
            "notification_projection",
            "reference_projection",
            "search_projection",
            "bulk_job_projection",
        ] {
            let definition = normalized_read
                .split(&format!("CREATE TABLE erpx_shared_read.{table}"))
                .nth(1)
                .expect("read table definition")
                .split("CREATE TABLE")
                .next()
                .expect("bounded read table definition");
            assert!(definition.contains("PRIMARY KEY (tenant_id"));
            assert!(definition.contains("FOREIGN KEY (tenant_id, projection_name, generation)"));
        }
    }

    #[test]
    fn replay_requires_an_exact_completed_receipt_and_payload_conflicts_fail() {
        let hash = [7_u8; 32];
        assert_eq!(
            classify_receipt(&hash, "completed", &hash).expect("exact replay"),
            MutationDisposition::Replayed
        );
        assert!(classify_receipt(&hash, "reserved", &hash).is_err());
        assert!(classify_receipt(&hash, "completed", &[8_u8; 32]).is_err());
        assert_eq!(exact_next_version(4, 5).expect("next"), 5);
        assert!(exact_next_version(4, 6).is_err());
    }

    #[test]
    fn every_service_transaction_verifies_exact_tenant_and_active_generation() {
        let source = include_str!("shared_enterprise_services.rs");
        assert!(source.contains("SELECT set_config('app.tenant_id',$1,true)"));
        assert!(source.contains("current_setting('app.tenant_id',true)"));
        assert!(source.contains("active.tenant_id = $1"));
        assert!(source.contains("active.status = 'active'"));
        let candidate_fallback = ["status IN ('active',", "'building')"].concat();
        assert!(!source.contains(&candidate_fallback));
        assert!(!source.contains(concat!("BYPASS", "RLS")));
    }

    #[test]
    fn outbox_contract_is_opaque_and_mutation_evidence_is_atomic() {
        let source = include_str!("shared_enterprise_services.rs");
        for authority in ["command_receipts", "audit_log", "outbox_events"] {
            assert!(source.contains(authority));
        }
        assert!(source.contains("notification_delivery_attempts"));
        assert!(source.contains("signature_recipients"));
        let json_snapshot_lookup = ["payload", "->>"].concat();
        assert!(!source.contains(&json_snapshot_lookup));
        let outbox = source
            .split("The outbox fact is intentionally opaque")
            .nth(1)
            .expect("outbox section");
        assert!(!outbox.contains("notification.preview"));
    }

    #[test]
    fn signature_commit_publishes_owner_projection_and_directory_facts_atomically() {
        let source = include_str!("shared_enterprise_services.rs");
        let owner_fact_symbol = ["append_signature", "_owner_facts"].concat();
        assert_eq!(source.matches(&owner_fact_symbol).count(), 2);
        assert!(source.contains("legal.signature_envelope_changed"));
        assert!(source.contains("ITEM_DIRECTORY_OWNER_EVENT_TYPE"));
        assert!(source.contains("record_policy_id(&record)"));
        assert!(source.contains("VALUES ($1,$2,'legal'"));
    }

    #[tokio::test]
    async fn opt_in_disposable_postgres_denies_missing_and_wrong_tenant() {
        let Ok(url) = std::env::var("ERPX_DISPOSABLE_SHARED_WRITE_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url)
            .await
            .expect("disposable write database");
        assert_disposable_database(&pool).await;
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let notification_id = Uuid::new_v4();
        let mut transaction = pool.begin().await.expect("transaction");
        bind_uuid(&mut transaction, tenant_a).await;
        sqlx::query(
            "INSERT INTO erpx_shared_write.notifications
                (tenant_id,notification_id,scope_key,tenant_scope,source_id,
                 source_record_ref,recipient_ref,classification,preview,inbox_state,
                 aggregate_version,delivery_version,evidence_sha256,created_at,updated_at)
             VALUES ($1,$2,'test', '{}'::jsonb,'test','reference:item:test','test',
                     'internal',NULL,'unread',1,1,$3,clock_timestamp(),clock_timestamp())",
        )
        .bind(tenant_a)
        .bind(notification_id)
        .bind(format!("sha256:{}", "a".repeat(64)))
        .execute(&mut *transaction)
        .await
        .expect("insert tenant A notification");

        bind_uuid(&mut transaction, tenant_b).await;
        let wrong_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_shared_write.notifications
             WHERE tenant_id=$1 AND notification_id=$2",
        )
        .bind(tenant_a)
        .bind(notification_id)
        .fetch_one(&mut *transaction)
        .await
        .expect("wrong tenant query");
        assert_eq!(wrong_count, 0);

        let _: String = sqlx::query_scalar("SELECT set_config('app.tenant_id','',true)")
            .fetch_one(&mut *transaction)
            .await
            .expect("clear tenant");
        let missing_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_shared_write.notifications WHERE notification_id=$1",
        )
        .bind(notification_id)
        .fetch_one(&mut *transaction)
        .await
        .expect("missing tenant query");
        assert_eq!(missing_count, 0);
        transaction.rollback().await.expect("rollback only");
    }

    #[tokio::test]
    async fn opt_in_disposable_postgres_never_reads_candidate_generation() {
        let Ok(url) = std::env::var("ERPX_DISPOSABLE_SHARED_READ_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url)
            .await
            .expect("disposable read database");
        assert_disposable_database(&pool).await;
        let tenant_id = Uuid::new_v4();
        let building = Uuid::new_v4();
        let active = Uuid::new_v4();
        let notification_id = Uuid::new_v4();
        let mut transaction = pool.begin().await.expect("transaction");
        bind_uuid(&mut transaction, tenant_id).await;
        for (generation, status) in [(building, "building"), (active, "active")] {
            sqlx::query(
                "INSERT INTO erpx_cqrs_read.projection_generations
                    (tenant_id,projection_name,generation,status,source_sequence,created_at,
                     activated_at)
                 VALUES ($1,'shared.notifications',$2,$3,1,clock_timestamp(),
                         CASE WHEN $3='active' THEN clock_timestamp() ELSE NULL END)",
            )
            .bind(tenant_id)
            .bind(generation)
            .bind(status)
            .execute(&mut *transaction)
            .await
            .expect("insert generation");
            sqlx::query(
                "INSERT INTO erpx_shared_read.notification_projection
                    (tenant_id,projection_name,generation,partition_key,notification_id,
                     recipient_ref,scope_key,inbox_state,source_aggregate_version,
                     source_global_sequence,state,projected_at)
                 VALUES ($1,'shared.notifications',$2,'test',$3,'recipient','test','unread',
                         1,1,jsonb_build_object('source',$4),clock_timestamp())",
            )
            .bind(tenant_id)
            .bind(generation)
            .bind(notification_id)
            .bind(status)
            .execute(&mut *transaction)
            .await
            .expect("insert projection");
        }
        let source: String = sqlx::query_scalar(
            "SELECT projection.state->>'source'
             FROM erpx_cqrs_read.projection_generations active_generation
             JOIN erpx_shared_read.notification_projection projection
               ON projection.tenant_id=active_generation.tenant_id
              AND projection.projection_name=active_generation.projection_name
              AND projection.generation=active_generation.generation
             WHERE active_generation.tenant_id=$1
               AND active_generation.projection_name='shared.notifications'
               AND active_generation.status='active'
               AND projection.partition_key='test' AND projection.notification_id=$2",
        )
        .bind(tenant_id)
        .bind(notification_id)
        .fetch_one(&mut *transaction)
        .await
        .expect("active projection");
        assert_eq!(source, "active");
        transaction.rollback().await.expect("rollback only");
    }

    async fn assert_disposable_database(pool: &PgPool) {
        let name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(pool)
            .await
            .expect("database name");
        assert!(
            name.starts_with("erpx_test_") || name.contains("disposable"),
            "refusing shared-service certification against non-disposable database {name}"
        );
    }

    async fn bind_uuid(transaction: &mut Transaction<'_, Postgres>, tenant_id: Uuid) {
        let expected = tenant_id.to_string();
        let actual: String = sqlx::query_scalar("SELECT set_config('app.tenant_id',$1,true)")
            .bind(&expected)
            .fetch_one(&mut **transaction)
            .await
            .expect("tenant binding");
        assert_eq!(actual, expected);
    }
}
