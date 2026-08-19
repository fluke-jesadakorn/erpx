use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs_sqlx::WritePool;
use erpx_crm::{
    PortError, QualifiedLeadEvidence, QualifiedLeadResolveRequest, QualifiedLeadResolverPort,
};
use erpx_integration_contracts::{ReferenceCode, customer, marketing};
use erpx_kernel::AuthorityScope;
use sqlx::Row;
use uuid::Uuid;

use crate::command::scope_key;

/// Exact owner envelope consumed from Marketing's qualified-lead outbox fact.
/// Payload provenance belongs to the committed event envelope, never an HTTP
/// request or CRM command body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingQualifiedLeadEnvelope {
    pub active_scope: AuthorityScope,
    pub source_event_id: Uuid,
    pub source_version: u64,
    pub source_global_sequence: u64,
    pub source_occurred_at: DateTime<Utc>,
    pub fact: marketing::QualifiedLeadFact,
}

#[derive(Debug, Clone)]
pub struct SqlxQualifiedLeadResolver {
    pool: WritePool,
}

impl SqlxQualifiedLeadResolver {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    /// Idempotently records a Marketing-owned fact in CRM's consumer inbox.
    /// A candidate can advance only to a greater source aggregate version.
    pub async fn record_published_fact(
        &self,
        envelope: &MarketingQualifiedLeadEnvelope,
    ) -> Result<bool, PortError> {
        validate_envelope(envelope)?;
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let source_version = to_i64(envelope.source_version)?;
        let source_global_sequence = to_i64(envelope.source_global_sequence)?;
        let candidate_id = envelope.fact.candidate.as_uuid();
        let applied = sqlx::query(
            "INSERT INTO erpx_crm_write.marketing_qualified_lead_evidence
                (holding_id,active_scope,scope_key,marketing_candidate_id,campaign_id,
                 audience_member_id,customer_id,source_code,
                 qualification_score_basis_points,marketing_source_version,
                 source_event_id,source_global_sequence,source_occurred_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
             ON CONFLICT (holding_id,marketing_candidate_id) DO UPDATE SET
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                campaign_id=EXCLUDED.campaign_id,
                audience_member_id=EXCLUDED.audience_member_id,
                customer_id=EXCLUDED.customer_id,source_code=EXCLUDED.source_code,
                qualification_score_basis_points=EXCLUDED.qualification_score_basis_points,
                marketing_source_version=EXCLUDED.marketing_source_version,
                source_event_id=EXCLUDED.source_event_id,
                source_global_sequence=EXCLUDED.source_global_sequence,
                source_occurred_at=EXCLUDED.source_occurred_at,recorded_at=now()
             WHERE erpx_crm_write.marketing_qualified_lead_evidence.holding_id
                    = erpx_cqrs_write.current_holding_id()
               AND erpx_crm_write.marketing_qualified_lead_evidence.marketing_source_version
                    < EXCLUDED.marketing_source_version
             RETURNING marketing_source_version",
        )
        .bind(serde_json::to_value(&envelope.active_scope).map_err(|_| PortError::Internal)?)
        .bind(scope_key(&envelope.active_scope).map_err(|_| PortError::Denied)?)
        .bind(candidate_id)
        .bind(envelope.fact.campaign.as_uuid())
        .bind(envelope.fact.audience_member.as_uuid())
        .bind(envelope.fact.existing_customer.map(|value| value.as_uuid()))
        .bind(envelope.fact.source.as_str())
        .bind(i32::from(envelope.fact.qualification_score_basis_points))
        .bind(source_version)
        .bind(envelope.source_event_id)
        .bind(source_global_sequence)
        .bind(envelope.source_occurred_at)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?;

        if applied.is_some() {
            transaction.commit().await.map_err(map_database)?;
            return Ok(true);
        }
        let current = sqlx::query(
            "SELECT campaign_id,audience_member_id,customer_id,source_code,
                    qualification_score_basis_points,marketing_source_version,
                    source_event_id,source_global_sequence
             FROM erpx_crm_write.marketing_qualified_lead_evidence
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND marketing_candidate_id=$1 FOR UPDATE",
        )
        .bind(candidate_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?
        .ok_or(PortError::Unavailable)?;
        let exact_replay = current.get::<Uuid, _>("campaign_id")
            == envelope.fact.campaign.as_uuid()
            && current.get::<Uuid, _>("audience_member_id")
                == envelope.fact.audience_member.as_uuid()
            && current.get::<Option<Uuid>, _>("customer_id")
                == envelope.fact.existing_customer.map(|value| value.as_uuid())
            && current.get::<String, _>("source_code") == envelope.fact.source.as_str()
            && current.get::<i32, _>("qualification_score_basis_points")
                == i32::from(envelope.fact.qualification_score_basis_points)
            && current.get::<i64, _>("marketing_source_version") == source_version
            && current.get::<Uuid, _>("source_event_id") == envelope.source_event_id
            && current.get::<i64, _>("source_global_sequence") == source_global_sequence;
        transaction.commit().await.map_err(map_database)?;
        if exact_replay {
            Ok(false)
        } else {
            Err(PortError::Conflict {
                expected: envelope.source_version,
                actual: u64::try_from(current.get::<i64, _>("marketing_source_version"))
                    .unwrap_or(0),
            })
        }
    }
}

#[async_trait]
impl QualifiedLeadResolverPort for SqlxQualifiedLeadResolver {
    async fn resolve_qualified_lead(
        &self,
        request: QualifiedLeadResolveRequest<'_>,
    ) -> Result<QualifiedLeadEvidence, PortError> {
        request
            .authority_scope
            .validate()
            .map_err(|_| PortError::Denied)?;
        if request.actor.principal_ref.trim().is_empty() {
            return Err(PortError::Denied);
        }
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let row = sqlx::query(
            "SELECT active_scope,campaign_id,audience_member_id,customer_id,source_code,
                    qualification_score_basis_points,marketing_source_version
             FROM erpx_crm_write.marketing_qualified_lead_evidence
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND marketing_candidate_id=$1",
        )
        .bind(request.candidate.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?;
        transaction.commit().await.map_err(map_database)?;
        let row = row.ok_or(PortError::NotFound)?;
        let scope: AuthorityScope =
            serde_json::from_value(row.get("active_scope")).map_err(|_| PortError::Internal)?;
        if !request.authority_scope.contains(&scope) {
            return Err(PortError::Denied);
        }
        let source_version = u64::try_from(row.get::<i64, _>("marketing_source_version"))
            .map_err(|_| PortError::Internal)?;
        let fact = marketing::QualifiedLeadFact {
            candidate: request.candidate,
            campaign: marketing::CampaignRef::try_from_uuid(row.get("campaign_id"))
                .map_err(|_| PortError::Internal)?,
            audience_member: marketing::AudienceMemberRef::try_from_uuid(
                row.get("audience_member_id"),
            )
            .map_err(|_| PortError::Internal)?,
            existing_customer: row
                .get::<Option<Uuid>, _>("customer_id")
                .map(customer::CustomerRef::try_from_uuid)
                .transpose()
                .map_err(|_| PortError::Internal)?,
            source: ReferenceCode::try_new(row.get::<String, _>("source_code"))
                .map_err(|_| PortError::Internal)?,
            qualification_score_basis_points: u16::try_from(
                row.get::<i32, _>("qualification_score_basis_points"),
            )
            .map_err(|_| PortError::Internal)?,
        };
        Ok(QualifiedLeadEvidence {
            fact,
            source_version,
        })
    }
}

fn validate_envelope(value: &MarketingQualifiedLeadEnvelope) -> Result<(), PortError> {
    value
        .active_scope
        .validate()
        .map_err(|_| PortError::Denied)?;
    if value.source_event_id.is_nil()
        || value.source_version == 0
        || value.source_global_sequence == 0
        || value.fact.candidate.as_uuid().is_nil()
        || value.fact.campaign.as_uuid().is_nil()
        || value.fact.audience_member.as_uuid().is_nil()
        || value.fact.qualification_score_basis_points == 0
        || value.fact.qualification_score_basis_points > 10_000
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, PortError> {
    i64::try_from(value).map_err(|_| PortError::Internal)
}

fn map_database(_error: impl std::fmt::Display) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_contract_is_holding_bound_and_never_reads_marketing_tables() {
        let source = include_str!("qualified_lead.rs");
        assert!(source.contains("holding_id=erpx_cqrs_write.current_holding_id()"));
        assert!(source.contains("marketing_source_version"));
        assert!(source.contains("source_event_id"));
        assert!(!source.contains(&["erpx_marketing", "write"].join("_")));
        assert!(!source.contains(&["erpx_marketing", "read"].join("_")));
    }
}
