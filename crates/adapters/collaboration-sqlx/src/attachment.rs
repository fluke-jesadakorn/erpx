//! Holding-local attachment registry. Object identity remains private to this
//! adapter and every public operation binds the exact authorized record,
//! optional thread, uploader, classification, and current lifecycle in SQL.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_collaboration::{
    AttachmentId, AttachmentLifecycle, AttachmentMutationReceipt, AttachmentRecord, AttachmentRef,
    AttachmentRegistryPort, AttachmentView, AttachmentWorkerTransition, AuthorizedAttachmentAccess,
    AuthorizedThreadAccess, CollaborationAction, MutationIdentity, PortError, PrincipalRef,
    ThreadId,
};
use erpx_kernel::{ActorKind, DataClassification, HoldingId, PrincipalId, PrincipalKind};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SqlxAttachmentRegistry {
    pool: PgPool,
    holding_id: HoldingId,
}

pub fn attachment_object_key(
    holding_id: HoldingId,
    attachment_id: AttachmentId,
    storage_object_id: Uuid,
) -> String {
    format!(
        "collaboration/{}/{}/{}",
        holding_id,
        attachment_id.as_uuid(),
        storage_object_id
    )
}

impl SqlxAttachmentRegistry {
    pub const fn for_holding(pool: PgPool, holding_id: HoldingId) -> Self {
        Self { pool, holding_id }
    }

    fn validate_access(
        &self,
        access: &AuthorizedAttachmentAccess,
        expected: CollaborationAction,
    ) -> Result<(Uuid, Uuid), PortError> {
        if access.holding_id() != self.holding_id
            || access.action() != expected
            || access.actor().kind() != PrincipalKind::Human
            || !matches!(
                access.attribution().kind,
                ActorKind::HoldingUser | ActorKind::PlatformUser
            )
            || access.attribution().represented_principal_ref.is_some()
            || access.attribution().support_capability_id.is_some()
        {
            return Err(PortError::Denied);
        }
        let actor_id =
            Uuid::parse_str(&access.attribution().principal_ref).map_err(|_| PortError::Denied)?;
        if actor_id != access.actor().id().as_uuid() {
            return Err(PortError::Denied);
        }
        let session_id = access
            .attribution()
            .session_id
            .map(|value| value.as_uuid())
            .ok_or(PortError::Denied)?;
        Ok((actor_id, session_id))
    }

    async fn begin(&self) -> Result<Transaction<'_, Postgres>, PortError> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        set_holding(&mut tx, self.holding_id).await?;
        Ok(tx)
    }
}

#[async_trait]
impl AttachmentRegistryPort for SqlxAttachmentRegistry {
    async fn prepare_upload(
        &self,
        access: &AuthorizedAttachmentAccess,
        identity: &MutationIdentity,
        attachment: &AttachmentRecord,
    ) -> Result<AttachmentMutationReceipt, PortError> {
        let (actor_id, session_id) =
            self.validate_access(access, CollaborationAction::PrepareAttachmentUpload)?;
        identity.validate()?;
        attachment.validate().map_err(|_| PortError::InvalidState)?;
        if attachment.holding_id != self.holding_id
            || &attachment.anchor != access.anchor()
            || attachment.thread_id != access.thread_id()
            || attachment.uploader != access.actor()
            || attachment.classification != access.classification()
            || attachment.lifecycle != AttachmentLifecycle::PendingUpload
        {
            return Err(PortError::Denied);
        }
        let mut tx = self.begin().await?;
        if let Some(receipt) = load_receipt(&mut tx, self.holding_id, identity).await? {
            let attachment = load_bound(&mut tx, access, receipt).await?;
            tx.commit().await.map_err(database)?;
            return Ok(AttachmentMutationReceipt {
                attachment,
                replayed: true,
            });
        }
        let event_id = Uuid::new_v4();
        let object_key = attachment_object_key(
            self.holding_id,
            attachment.attachment_id,
            attachment.storage_object_id,
        );
        let document = serde_json::to_value(attachment).map_err(|_| PortError::InvalidState)?;
        let changed = sqlx::query(
            "INSERT INTO erpx_collaboration.attachments
             (holding_id,attachment_id,object_id,object_key,anchor_record_id,anchor_record_ref,
              thread_id,uploader_principal_id,uploader_session_id,classification,file_name,
              media_type,expected_size_bytes,expected_digest,verified_size_bytes,verified_digest,
              lifecycle,lifecycle_version,upload_expires_at,finalized_at,scanned_at,scan_engine,
              scan_evidence_digest,quarantine_reason,expires_at,created_at,updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,NULL,NULL,
                     'pending_upload',1,$15,NULL,NULL,NULL,NULL,NULL,$16,$17,$17)",
        )
        .bind(self.holding_id.as_uuid())
        .bind(attachment.attachment_id.as_uuid())
        .bind(attachment.storage_object_id)
        .bind(object_key)
        .bind(attachment.anchor.id().as_uuid())
        .bind(serde_json::to_value(&attachment.anchor).map_err(|_| PortError::InvalidState)?)
        .bind(attachment.thread_id.map(|value| value.as_uuid()))
        .bind(actor_id)
        .bind(session_id)
        .bind(classification_name(attachment.classification))
        .bind(&attachment.file_name)
        .bind(&attachment.media_type)
        .bind(to_i64(attachment.size_bytes)?)
        .bind(attachment.digest.to_vec())
        .bind(attachment.upload_expires_at)
        .bind(attachment.expires_at)
        .bind(attachment.created_at)
        .execute(&mut *tx)
        .await
        .map_err(database)?
        .rows_affected();
        if changed != 1 {
            return Err(PortError::Conflict);
        }
        insert_event(
            &mut tx,
            self.holding_id,
            event_id,
            attachment.attachment_id,
            1,
            AttachmentLifecycle::PendingUpload,
            &document,
            Some(actor_id),
            Some(session_id),
            Some(identity),
            attachment.created_at,
        )
        .await?;
        insert_receipt(
            &mut tx,
            self.holding_id,
            identity,
            attachment.attachment_id,
            1,
            event_id,
            attachment.created_at,
        )
        .await?;
        tx.commit().await.map_err(database)?;
        Ok(AttachmentMutationReceipt {
            attachment: attachment.clone(),
            replayed: false,
        })
    }

    async fn finalize_upload(
        &self,
        access: &AuthorizedAttachmentAccess,
        identity: &MutationIdentity,
        attachment_id: AttachmentId,
        verified_size_bytes: u64,
        verified_digest: [u8; 32],
        at: DateTime<Utc>,
    ) -> Result<AttachmentMutationReceipt, PortError> {
        let (actor_id, session_id) =
            self.validate_access(access, CollaborationAction::FinalizeAttachmentUpload)?;
        identity.validate()?;
        if verified_size_bytes == 0 || verified_digest == [0; 32] {
            return Err(PortError::InvalidState);
        }
        let mut tx = self.begin().await?;
        if let Some(receipt) = load_receipt(&mut tx, self.holding_id, identity).await? {
            if receipt != attachment_id {
                return Err(PortError::Conflict);
            }
            let attachment = load_bound(&mut tx, access, receipt).await?;
            tx.commit().await.map_err(database)?;
            return Ok(AttachmentMutationReceipt {
                attachment,
                replayed: true,
            });
        }
        let anchor = serde_json::to_value(access.anchor()).map_err(|_| PortError::InvalidState)?;
        let row = sqlx::query(
            "SELECT expected_size_bytes,expected_digest
             FROM erpx_collaboration.attachments
             WHERE holding_id=$1 AND attachment_id=$2 AND anchor_record_id=$3
               AND anchor_record_ref=$4::JSONB
               AND (thread_id IS NOT DISTINCT FROM $5 OR (thread_id IS NULL AND $5 IS NOT NULL))
               AND uploader_principal_id=$6 AND uploader_session_id=$7
               AND classification=$8 AND lifecycle='pending_upload'
               AND upload_expires_at>$9 FOR UPDATE",
        )
        .bind(self.holding_id.as_uuid())
        .bind(attachment_id.as_uuid())
        .bind(access.anchor().id().as_uuid())
        .bind(anchor)
        .bind(access.thread_id().map(|value| value.as_uuid()))
        .bind(actor_id)
        .bind(session_id)
        .bind(classification_name(access.classification()))
        .bind(at)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        .ok_or(PortError::Denied)?;
        let expected_size: i64 = row
            .try_get("expected_size_bytes")
            .map_err(|_| PortError::Internal)?;
        let expected_digest: Vec<u8> = row
            .try_get("expected_digest")
            .map_err(|_| PortError::Internal)?;
        if to_u64(expected_size)? != verified_size_bytes
            || expected_digest.as_slice() != verified_digest
        {
            return Err(PortError::Conflict);
        }
        let changed = sqlx::query(
            "UPDATE erpx_collaboration.attachments
             SET verified_size_bytes=$3,verified_digest=$4,thread_id=$5,lifecycle='pending_scan',
                 lifecycle_version=2,finalized_at=$6,updated_at=$6
             WHERE holding_id=$1 AND attachment_id=$2 AND lifecycle='pending_upload'
             RETURNING attachment_id",
        )
        .bind(self.holding_id.as_uuid())
        .bind(attachment_id.as_uuid())
        .bind(to_i64(verified_size_bytes)?)
        .bind(verified_digest.to_vec())
        .bind(access.thread_id().map(|value| value.as_uuid()))
        .bind(at)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if changed.is_none() {
            return Err(PortError::Conflict);
        }
        let attachment = load_bound(&mut tx, access, attachment_id).await?;
        let document = serde_json::to_value(&attachment).map_err(|_| PortError::Internal)?;
        let event_id = Uuid::new_v4();
        insert_event(
            &mut tx,
            self.holding_id,
            event_id,
            attachment_id,
            2,
            AttachmentLifecycle::PendingScan,
            &document,
            Some(actor_id),
            Some(session_id),
            Some(identity),
            at,
        )
        .await?;
        insert_receipt(
            &mut tx,
            self.holding_id,
            identity,
            attachment_id,
            2,
            event_id,
            at,
        )
        .await?;
        tx.commit().await.map_err(database)?;
        Ok(AttachmentMutationReceipt {
            attachment,
            replayed: false,
        })
    }

    async fn transition(
        &self,
        attachment_id: AttachmentId,
        expected: AttachmentLifecycle,
        transition: AttachmentWorkerTransition,
        at: DateTime<Utc>,
    ) -> Result<AttachmentRecord, PortError> {
        transition.validate()?;
        let next = transition.lifecycle();
        if !matches!(
            (&transition, expected),
            (
                AttachmentWorkerTransition::ScanPassed { .. }
                    | AttachmentWorkerTransition::ScanRejected { .. },
                AttachmentLifecycle::PendingScan
            ) | (
                AttachmentWorkerTransition::Expire,
                AttachmentLifecycle::PendingUpload | AttachmentLifecycle::PendingScan
            )
        ) {
            return Err(PortError::InvalidState);
        }
        let mut tx = self.begin().await?;
        let row = sqlx::query(
            "SELECT lifecycle_version FROM erpx_collaboration.attachments
             WHERE holding_id=$1 AND attachment_id=$2 AND lifecycle=$3 FOR UPDATE",
        )
        .bind(self.holding_id.as_uuid())
        .bind(attachment_id.as_uuid())
        .bind(lifecycle_name(expected))
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        .ok_or(PortError::Conflict)?;
        let current_version: i64 = row
            .try_get("lifecycle_version")
            .map_err(|_| PortError::Internal)?;
        let next_version = current_version.checked_add(1).ok_or(PortError::Internal)?;
        let (scanned_at, scan_engine, evidence, quarantine_reason) = match &transition {
            AttachmentWorkerTransition::ScanPassed {
                engine,
                evidence_digest,
            } => (
                Some(at),
                Some(engine.as_str()),
                Some(evidence_digest.to_vec()),
                None,
            ),
            AttachmentWorkerTransition::ScanRejected {
                engine,
                evidence_digest,
                reason,
            } => (
                Some(at),
                Some(engine.as_str()),
                Some(evidence_digest.to_vec()),
                Some(reason.as_str()),
            ),
            AttachmentWorkerTransition::Expire => (None, None, None, None),
        };
        sqlx::query(
            "UPDATE erpx_collaboration.attachments
             SET lifecycle=$3,lifecycle_version=$4,scanned_at=$5,scan_engine=$6,
                 scan_evidence_digest=$7,quarantine_reason=$8,updated_at=$9
             WHERE holding_id=$1 AND attachment_id=$2 AND lifecycle=$10",
        )
        .bind(self.holding_id.as_uuid())
        .bind(attachment_id.as_uuid())
        .bind(lifecycle_name(next))
        .bind(next_version)
        .bind(scanned_at)
        .bind(scan_engine)
        .bind(evidence)
        .bind(quarantine_reason)
        .bind(at)
        .bind(lifecycle_name(expected))
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        let attachment = load_by_id(&mut tx, self.holding_id, attachment_id).await?;
        let document = serde_json::to_value(&attachment).map_err(|_| PortError::Internal)?;
        insert_event(
            &mut tx,
            self.holding_id,
            Uuid::new_v4(),
            attachment_id,
            next_version,
            next,
            &document,
            None,
            None,
            None,
            at,
        )
        .await?;
        tx.commit().await.map_err(database)?;
        Ok(attachment)
    }

    async fn load_ready_for_comment(
        &self,
        access: &AuthorizedThreadAccess,
        uploader: PrincipalRef,
        classification: DataClassification,
        attachments: &[AttachmentRef],
        now: DateTime<Utc>,
    ) -> Result<Vec<AttachmentView>, PortError> {
        if access.holding_id() != self.holding_id
            || uploader.kind() != PrincipalKind::Human
            || attachments.len() > 64
        {
            return Err(PortError::Denied);
        }
        if attachments.is_empty() {
            return Ok(Vec::new());
        }
        let ids = attachments
            .iter()
            .map(|value| value.attachment_id.as_uuid())
            .collect::<Vec<_>>();
        let mut tx = self.begin().await?;
        let rows = sqlx::query(
            "SELECT attachment.*
             FROM erpx_collaboration.threads thread
             JOIN erpx_collaboration.thread_anchors anchor
               ON anchor.holding_id=thread.holding_id AND anchor.thread_id=thread.thread_id
             JOIN erpx_collaboration.thread_policies policy
               ON policy.holding_id=thread.holding_id AND policy.thread_id=thread.thread_id
             JOIN erpx_collaboration.attachments attachment
               ON attachment.holding_id=thread.holding_id
              AND attachment.anchor_record_id=thread.anchor_record_id
              AND attachment.anchor_record_ref=anchor.anchor_document->'record'
              AND attachment.thread_id=thread.thread_id
             WHERE thread.holding_id=$1 AND thread.thread_id=$2 AND thread.anchor_record_id=$3
               AND policy.policy_document->'visibility_scope'=$4::JSONB
               AND attachment.attachment_id=ANY($5)
               AND attachment.uploader_principal_id=$6 AND attachment.classification=$7
               AND attachment.lifecycle='ready'
               AND (attachment.expires_at IS NULL OR attachment.expires_at>$8)
             ORDER BY attachment.attachment_id",
        )
        .bind(self.holding_id.as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(access.anchor_record_id().as_uuid())
        .bind(serde_json::to_value(access.visibility_scope()).map_err(|_| PortError::InvalidState)?)
        .bind(&ids)
        .bind(uploader.id().as_uuid())
        .bind(classification_name(classification))
        .bind(now)
        .fetch_all(&mut *tx)
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;
        if rows.len() != attachments.len() {
            return Err(PortError::Denied);
        }
        let records = rows
            .iter()
            .map(record_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        for requested in attachments {
            let stored = records
                .iter()
                .find(|value| value.attachment_id == requested.attachment_id)
                .ok_or(PortError::Denied)?;
            if stored.file_name != requested.file_name
                || stored.media_type != requested.media_type
                || stored.classification != requested.classification
            {
                return Err(PortError::Denied);
            }
        }
        Ok(records.iter().map(AttachmentView::from).collect())
    }

    async fn get_for_download(
        &self,
        access: &AuthorizedAttachmentAccess,
        attachment_id: AttachmentId,
        now: DateTime<Utc>,
    ) -> Result<Option<AttachmentRecord>, PortError> {
        self.validate_access(access, CollaborationAction::DownloadAttachment)?;
        let mut tx = self.begin().await?;
        let result = load_bound_optional(&mut tx, access, attachment_id).await?;
        tx.commit().await.map_err(database)?;
        Ok(result.filter(|value| {
            value.lifecycle == AttachmentLifecycle::Ready
                && value.expires_at.is_none_or(|expires_at| expires_at > now)
        }))
    }
}

async fn load_receipt(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    identity: &MutationIdentity,
) -> Result<Option<AttachmentId>, PortError> {
    let row = sqlx::query(
        "SELECT request_digest,attachment_id
         FROM erpx_collaboration.attachment_command_receipts
         WHERE holding_id=$1 AND idempotency_key=$2",
    )
    .bind(holding_id.as_uuid())
    .bind(&identity.idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(database)?;
    let Some(row) = row else { return Ok(None) };
    let digest: Vec<u8> = row
        .try_get("request_digest")
        .map_err(|_| PortError::Internal)?;
    if digest.as_slice() != identity.request_digest {
        return Err(PortError::Conflict);
    }
    AttachmentId::try_from_uuid(
        row.try_get("attachment_id")
            .map_err(|_| PortError::Internal)?,
    )
    .map(Some)
    .map_err(|_| PortError::Internal)
}

async fn insert_receipt(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    identity: &MutationIdentity,
    attachment_id: AttachmentId,
    lifecycle_version: i64,
    event_id: Uuid,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_collaboration.attachment_command_receipts
         (holding_id,idempotency_key,command_id,request_digest,attachment_id,
          lifecycle_version,event_id,created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(holding_id.as_uuid())
    .bind(&identity.idempotency_key)
    .bind(identity.command_id.as_uuid())
    .bind(identity.request_digest.to_vec())
    .bind(attachment_id.as_uuid())
    .bind(lifecycle_version)
    .bind(event_id)
    .bind(at)
    .execute(&mut **tx)
    .await
    .map_err(database)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    event_id: Uuid,
    attachment_id: AttachmentId,
    lifecycle_version: i64,
    lifecycle: AttachmentLifecycle,
    document: &serde_json::Value,
    actor_id: Option<Uuid>,
    session_id: Option<Uuid>,
    identity: Option<&MutationIdentity>,
    at: DateTime<Utc>,
) -> Result<(), PortError> {
    sqlx::query(
        "INSERT INTO erpx_collaboration.attachment_events
         (holding_id,event_id,attachment_id,lifecycle_version,lifecycle,event_document,
          actor_principal_id,actor_session_id,command_id,correlation_id,occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(holding_id.as_uuid())
    .bind(event_id)
    .bind(attachment_id.as_uuid())
    .bind(lifecycle_version)
    .bind(lifecycle_name(lifecycle))
    .bind(document)
    .bind(actor_id)
    .bind(session_id)
    .bind(identity.map(|value| value.command_id.as_uuid()))
    .bind(identity.map(|value| value.correlation_id.as_uuid()))
    .bind(at)
    .execute(&mut **tx)
    .await
    .map_err(database)?;
    Ok(())
}

async fn load_bound(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedAttachmentAccess,
    attachment_id: AttachmentId,
) -> Result<AttachmentRecord, PortError> {
    load_bound_optional(tx, access, attachment_id)
        .await?
        .ok_or(PortError::Denied)
}

async fn load_bound_optional(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedAttachmentAccess,
    attachment_id: AttachmentId,
) -> Result<Option<AttachmentRecord>, PortError> {
    let row = sqlx::query(
        "SELECT * FROM erpx_collaboration.attachments
         WHERE holding_id=$1 AND attachment_id=$2 AND anchor_record_id=$3
           AND anchor_record_ref=$4::JSONB AND thread_id IS NOT DISTINCT FROM $5
           AND classification=$6",
    )
    .bind(access.holding_id().as_uuid())
    .bind(attachment_id.as_uuid())
    .bind(access.anchor().id().as_uuid())
    .bind(serde_json::to_value(access.anchor()).map_err(|_| PortError::InvalidState)?)
    .bind(access.thread_id().map(|value| value.as_uuid()))
    .bind(classification_name(access.classification()))
    .fetch_optional(&mut **tx)
    .await
    .map_err(database)?;
    row.as_ref().map(record_from_row).transpose()
}

async fn load_by_id(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
    attachment_id: AttachmentId,
) -> Result<AttachmentRecord, PortError> {
    let row = sqlx::query(
        "SELECT * FROM erpx_collaboration.attachments
         WHERE holding_id=$1 AND attachment_id=$2",
    )
    .bind(holding_id.as_uuid())
    .bind(attachment_id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(database)?
    .ok_or(PortError::Denied)?;
    record_from_row(&row)
}

fn record_from_row(row: &sqlx::postgres::PgRow) -> Result<AttachmentRecord, PortError> {
    let attachment = AttachmentRecord {
        attachment_id: AttachmentId::try_from_uuid(
            row.try_get("attachment_id")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        storage_object_id: row.try_get("object_id").map_err(|_| PortError::Internal)?,
        holding_id: HoldingId::try_from_uuid(
            row.try_get("holding_id").map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        anchor: serde_json::from_value(
            row.try_get("anchor_record_ref")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        thread_id: row
            .try_get::<Option<Uuid>, _>("thread_id")
            .map_err(|_| PortError::Internal)?
            .map(ThreadId::try_from_uuid)
            .transpose()
            .map_err(|_| PortError::Internal)?,
        uploader: PrincipalRef::new(
            PrincipalId::try_from_uuid(
                row.try_get("uploader_principal_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            PrincipalKind::Human,
        ),
        classification: classification(
            row.try_get("classification")
                .map_err(|_| PortError::Internal)?,
        )?,
        file_name: row.try_get("file_name").map_err(|_| PortError::Internal)?,
        media_type: row.try_get("media_type").map_err(|_| PortError::Internal)?,
        size_bytes: to_u64(
            row.try_get("expected_size_bytes")
                .map_err(|_| PortError::Internal)?,
        )?,
        digest: digest(
            row.try_get("expected_digest")
                .map_err(|_| PortError::Internal)?,
        )?,
        lifecycle: lifecycle(row.try_get("lifecycle").map_err(|_| PortError::Internal)?)?,
        created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        upload_expires_at: row
            .try_get("upload_expires_at")
            .map_err(|_| PortError::Internal)?,
        expires_at: row.try_get("expires_at").map_err(|_| PortError::Internal)?,
    };
    attachment.validate().map_err(|_| PortError::Internal)?;
    Ok(attachment)
}

fn digest(value: Vec<u8>) -> Result<[u8; 32], PortError> {
    value.try_into().map_err(|_| PortError::Internal)
}

const fn classification_name(value: DataClassification) -> &'static str {
    match value {
        DataClassification::Public => "public",
        DataClassification::Internal => "internal",
        DataClassification::Confidential => "confidential",
        DataClassification::Restricted => "restricted",
    }
}

fn classification(value: &str) -> Result<DataClassification, PortError> {
    match value {
        "public" => Ok(DataClassification::Public),
        "internal" => Ok(DataClassification::Internal),
        "confidential" => Ok(DataClassification::Confidential),
        "restricted" => Ok(DataClassification::Restricted),
        _ => Err(PortError::Internal),
    }
}

const fn lifecycle_name(value: AttachmentLifecycle) -> &'static str {
    match value {
        AttachmentLifecycle::PendingUpload => "pending_upload",
        AttachmentLifecycle::PendingScan => "pending_scan",
        AttachmentLifecycle::Ready => "ready",
        AttachmentLifecycle::Quarantined => "quarantined",
        AttachmentLifecycle::Expired => "expired",
    }
}

fn lifecycle(value: &str) -> Result<AttachmentLifecycle, PortError> {
    match value {
        "pending_upload" => Ok(AttachmentLifecycle::PendingUpload),
        "pending_scan" => Ok(AttachmentLifecycle::PendingScan),
        "ready" => Ok(AttachmentLifecycle::Ready),
        "quarantined" => Ok(AttachmentLifecycle::Quarantined),
        "expired" => Ok(AttachmentLifecycle::Expired),
        _ => Err(PortError::Internal),
    }
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: HoldingId,
) -> Result<(), PortError> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id',$1,true)")
        .bind(&expected)
        .fetch_one(&mut **tx)
        .await
        .map_err(database)?;
    (bound == expected).then_some(()).ok_or(PortError::Denied)
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::InvalidState)
}

fn to_u64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn database(error: sqlx::Error) -> PortError {
    if let sqlx::Error::Database(database) = &error
        && database.code().as_deref() == Some("23505")
    {
        return PortError::Conflict;
    }
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    #[test]
    fn registry_binds_every_security_dimension_and_never_accepts_object_keys() {
        let source = include_str!("attachment.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for binding in [
            "anchor_record_ref=$4::JSONB",
            "thread_id IS NOT DISTINCT FROM $5",
            "uploader_principal_id=$6",
            "uploader_session_id=$7",
            "classification=$8",
            "expected_digest.as_slice() != verified_digest",
        ] {
            assert!(
                production.contains(binding),
                "missing SQL binding: {binding}"
            );
        }
        assert!(production.contains("storage_object_id"));
        assert!(!production.contains("command.object_key"));
    }

    #[test]
    fn only_the_trusted_transition_path_can_mark_ready() {
        let source = include_str!("attachment.rs");
        let finalize = source
            .split("async fn finalize_upload")
            .nth(1)
            .unwrap()
            .split("async fn transition")
            .next()
            .unwrap();
        assert!(finalize.contains("lifecycle='pending_scan'"));
        assert!(!finalize.contains("lifecycle='ready'"));
        let transition = source
            .split("async fn transition")
            .nth(1)
            .unwrap()
            .split("async fn load_ready_for_comment")
            .next()
            .unwrap();
        assert!(transition.contains("AttachmentWorkerTransition::ScanPassed"));
        assert!(transition.contains("evidence_digest.to_vec()"));
        assert!(!transition.contains("vec![1_u8; 32]"));
    }
}
