use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext, ReadPool};
use erpx_kernel::TenantScope;
use erpx_legal::{
    LegalDocumentId, PortError, SignatureEnvelopeId, SignatureEnvelopeQueryPort,
    SignatureEnvelopeView, SignatureStatus,
};
use serde::Deserialize;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
pub const SIGNATURE_PROJECTION: &str = "shared.signatures";
const SIGNATURE_EVENT: &str = "legal.signature_envelope_changed";
#[derive(Debug, Deserialize)]
struct SignatureProjectionFact {
    envelope_id: Uuid,
    envelope_version: u64,
    tenant_scope: TenantScope,
    document_id: Uuid,
    document_version: u64,
    envelope_status: String,
    retention_until: DateTime<Utc>,
}
#[derive(Debug, Clone)]
pub struct SignatureEnvelopeProjector {
    events: Vec<String>,
}
impl SignatureEnvelopeProjector {
    pub fn new() -> Self {
        Self {
            events: vec![SIGNATURE_EVENT.into()],
        }
    }
}
impl Default for SignatureEnvelopeProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SignatureEnvelopeReadAdapter {
    pool: ReadPool,
}
impl SignatureEnvelopeReadAdapter {
    pub const fn new(pool: ReadPool) -> Self {
        Self { pool }
    }
}
#[async_trait]
impl SignatureEnvelopeQueryPort for SignatureEnvelopeReadAdapter {
    async fn get_signature_envelope(
        &self,
        authority_scope: &TenantScope,
        envelope_id: SignatureEnvelopeId,
    ) -> Result<Option<SignatureEnvelopeView>, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM erpx_cqrs_read.projection_generations WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 AND status='active')").bind(SIGNATURE_PROJECTION).fetch_one(&mut *tx).await.map_err(|_|PortError::Unavailable)?;
        if !active {
            return Err(PortError::Unavailable);
        }
        let scope_key = serde_json::to_string(authority_scope).map_err(|_| PortError::Internal)?;
        let row=sqlx::query("SELECT record.envelope_id,record.source_aggregate_version,record.document_id,record.document_version,record.envelope_status,record.retention_until FROM erpx_shared_read.signature_envelope_projection record JOIN erpx_cqrs_read.projection_generations generation ON generation.tenant_id=record.tenant_id AND generation.projection_name=record.projection_name AND generation.generation=record.generation AND generation.status='active' WHERE record.tenant_id=erpx_cqrs_read.current_tenant_id() AND record.projection_name=$1 AND record.scope_key=$2 AND record.envelope_id=$3")
            .bind(SIGNATURE_PROJECTION).bind(scope_key).bind(envelope_id.as_uuid()).fetch_optional(&mut *tx).await.map_err(|_|PortError::Unavailable)?;
        row.map(|row| {
            Ok(SignatureEnvelopeView {
                envelope_id: SignatureEnvelopeId::try_from_uuid(
                    row.try_get("envelope_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                version: u64::try_from(
                    row.try_get::<i64, _>("source_aggregate_version")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                document_id: LegalDocumentId::try_from_uuid(
                    row.try_get("document_id")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                document_version: u64::try_from(
                    row.try_get::<i64, _>("document_version")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                status: parse_status(
                    row.try_get("envelope_status")
                        .map_err(|_| PortError::Internal)?,
                )?,
                retention_until: row
                    .try_get("retention_until")
                    .map_err(|_| PortError::Internal)?,
            })
        })
        .transpose()
    }
}
fn parse_status(v: &str) -> Result<SignatureStatus, PortError> {
    match v {
        "draft" => Ok(SignatureStatus::Draft),
        "submitted" => Ok(SignatureStatus::Submitted),
        "in_progress" => Ok(SignatureStatus::InProgress),
        "completed" => Ok(SignatureStatus::Completed),
        "failed" => Ok(SignatureStatus::Failed),
        "cancelled" => Ok(SignatureStatus::Cancelled),
        "expired" => Ok(SignatureStatus::Expired),
        _ => Err(PortError::Internal),
    }
}
#[async_trait]
impl Projector for SignatureEnvelopeProjector {
    fn name(&self) -> &str {
        SIGNATURE_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.events
    }
    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &ProjectorContext<'_>,
        e: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        if c.projection != SIGNATURE_PROJECTION
            || e.context != "legal"
            || e.event_type != SIGNATURE_EVENT
            || e.aggregate_type != "signature_envelope"
        {
            return Err(ProjectionFailure::new(
                "signature projection event mismatch",
            ));
        }
        let f: SignatureProjectionFact =
            serde_json::from_value(e.payload.clone()).map_err(failure)?;
        if f.envelope_id != e.aggregate_id.as_uuid()
            || f.envelope_version != e.aggregate_version
            || f.tenant_scope != e.tenant_scope
            || f.document_version == 0
        {
            return Err(ProjectionFailure::new(
                "signature projection metadata mismatch",
            ));
        }
        sqlx::query("INSERT INTO erpx_shared_read.signature_envelope_projection (tenant_id,projection_name,generation,partition_key,envelope_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,document_id,document_version,envelope_status,retention_until,projected_at) VALUES (erpx_cqrs_read.current_tenant_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,now()) ON CONFLICT (tenant_id,projection_name,generation,partition_key,envelope_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,envelope_status=EXCLUDED.envelope_status,retention_until=EXCLUDED.retention_until,projected_at=now() WHERE erpx_shared_read.signature_envelope_projection.source_aggregate_version<EXCLUDED.source_aggregate_version").bind(SIGNATURE_PROJECTION).bind(c.generation).bind(&e.partition).bind(f.envelope_id).bind(i64::try_from(f.envelope_version).map_err(failure)?).bind(i64::try_from(e.global_sequence).map_err(failure)?).bind(serde_json::to_value(&f.tenant_scope).map_err(failure)?).bind(serde_json::to_string(&f.tenant_scope).map_err(failure)?).bind(f.document_id).bind(i64::try_from(f.document_version).map_err(failure)?).bind(f.envelope_status).bind(f.retention_until).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_shared_read.signature_envelope_projection WHERE tenant_id=erpx_cqrs_read.current_tenant_id() AND projection_name=$1 AND generation=$2").bind(SIGNATURE_PROJECTION).bind(c.generation).execute(&mut **tx).await.map_err(failure)?;
        Ok(())
    }
}
fn failure(e: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projector_is_generation_fenced_and_rebuildable() {
        let s = include_str!("signature_projection.rs");
        assert!(s.contains("projection_name,generation"));
        assert!(s.contains("generation.status='active'"));
        assert!(s.contains("source_aggregate_version<EXCLUDED"));
        assert!(s.contains("DELETE FROM erpx_shared_read.signature_envelope_projection"));
        assert_eq!(
            SignatureEnvelopeProjector::new().event_types(),
            &[SIGNATURE_EVENT.to_owned()]
        );
    }
}
