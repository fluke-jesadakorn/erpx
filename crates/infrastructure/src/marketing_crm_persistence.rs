//! SQLx write-side persistence for the Marketing-to-CRM boundary.
//!
//! The accepted-lead handoff is an immutable source binding.  Opportunities
//! are the only mutable CRM aggregate and may change only through a validated
//! domain lifecycle transition.  Attribution is recomputed from the stored
//! normalized opportunity record, so callers cannot submit fabricated revenue totals or
//! touch allocations.  Every mutation is routed through the holding-scoped
//! transaction helper and commits normalized source rows, outbox
//! events, and idempotency receipts together.

use erpx_domain::contracts::{AuthorityScope, CONTRACT_VERSION, CommandContext};
use erpx_domain::marketing_crm::{
    AcceptedLeadHandoff, AttributionModel, AttributionResult, CrmOpportunity, MarketingCrmError,
    deterministic_idempotency_key, validate_command_context,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

const HANDOFF_AGGREGATE: &str = "marketing_crm_accepted_lead_handoff";
const OPPORTUNITY_AGGREGATE: &str = "marketing_crm_opportunity";
const ATTRIBUTION_AGGREGATE: &str = "marketing_crm_attribution";
const HANDOFF_EVENT: &str = "marketing.crm.accepted_lead_handoff_saved.v1";
const OPPORTUNITY_EVENT: &str = "marketing.crm.opportunity_saved.v1";
const ATTRIBUTION_EVENT: &str = "marketing.crm.attribution_saved.v1";
const MAX_CONTEXT_KEY_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub reference: String,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrmAttributionWriteReceipt {
    pub record_id: Uuid,
    pub reference: String,
    pub model: String,
    pub source_opportunity_version: u64,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxMarketingCrmStore {
    write_pool: PgPool,
    data_profile_key: String,
}

impl SqlxMarketingCrmStore {
    pub fn new(
        write_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            data_profile_key,
        })
    }

    pub fn from_pool(
        write_pool: &PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), data_profile_key)
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Persist the immutable Marketing -> CRM acceptance boundary.
    ///
    /// A handoff is always version one.  Repeating the exact command returns
    /// its stored receipt; a different payload using the same key or reference
    /// fails closed.  `expected_version` is accepted for a common repository
    /// interface but must be absent for a new immutable handoff.
    pub async fn save_handoff(
        &self,
        handoff: &AcceptedLeadHandoff,
        context: &CommandContext,
        expected_version: Option<u64>,
    ) -> Result<CrmWriteReceipt, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let receipt = Self::persist_handoff_in_transaction(
            &mut tx,
            &self.data_profile_key,
            handoff,
            context,
            expected_version,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// The journey worker uses this transaction-local variant to couple a
    /// CRM handoff to the execution cursor advance and its outbox event. The
    /// caller owns the transaction and must commit all effects together.
    pub(crate) async fn persist_handoff_in_transaction(
        tx: &mut Transaction<'_, Postgres>,
        data_profile_key: &str,
        handoff: &AcceptedLeadHandoff,
        context: &CommandContext,
        expected_version: Option<u64>,
    ) -> Result<CrmWriteReceipt, PersistenceError> {
        validate_handoff_command(handoff, context)?;
        if expected_version.is_some() {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: HANDOFF_AGGREGATE.into(),
                aggregate_id: handoff.id,
                expected: expected_version,
                actual: None,
            });
        }
        let payload = serde_json::to_value(handoff)?;
        let request_hash = hash_request(&payload, expected_version)?;
        let (holding_id, company_id, branch_id) = scope_columns(&handoff.scope)?;

        lock_command_keys(
            tx,
            data_profile_key,
            HANDOFF_AGGREGATE,
            handoff.id,
            &handoff.reference,
            &handoff.idempotency_key,
        )
        .await?;
        if let Some(binding) =
            load_idempotency(&mut *tx, data_profile_key, &handoff.idempotency_key).await?
        {
            validate_idempotency_binding(
                &binding,
                HANDOFF_AGGREGATE,
                handoff.id,
                &handoff.reference,
                &request_hash,
                &handoff.idempotency_key,
            )?;
            let mut receipt: CrmWriteReceipt = serde_json::from_value(binding.response)?;
            validate_write_receipt(&receipt, HANDOFF_AGGREGATE, handoff.id, &handoff.reference)?;
            receipt.replayed = true;
            return Ok(receipt);
        }

        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT version
             FROM erpx_holding.crm_accepted_lead_handoffs
             WHERE data_profile_key=$1 AND handoff_id=$2
             FOR UPDATE",
        )
        .bind(data_profile_key)
        .bind(handoff.id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(version) = existing {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: HANDOFF_AGGREGATE.into(),
                aggregate_id: handoff.id,
                expected: None,
                actual: Some(as_u64(version, "CRM handoff version")?),
            });
        }
        reject_existing_reference(
            tx,
            data_profile_key,
            HANDOFF_AGGREGATE,
            &handoff.reference,
            handoff.id,
        )
        .await?;
        reject_existing_key_row(
            tx,
            data_profile_key,
            HANDOFF_AGGREGATE,
            &handoff.idempotency_key,
            handoff.id,
        )
        .await?;

        sqlx::query(
            "INSERT INTO erpx_holding.crm_accepted_lead_handoffs
                (handoff_id, data_profile_key, holding_id, company_id, branch_id, scope,
                 reference, idempotency_key, command_id, source, account_id,
                 account_scope, contact_id, contact_scope, accepted_at, version, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,1,$16)",
        )
        .bind(handoff.id)
        .bind(data_profile_key)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(serde_json::to_value(&handoff.scope)?)
        .bind(&handoff.reference)
        .bind(&handoff.idempotency_key)
        .bind(handoff.command_id)
        .bind(serde_json::to_value(&handoff.source)?)
        .bind(handoff.account.id.as_str())
        .bind(serde_json::to_value(&handoff.account.scope)?)
        .bind(handoff.contact.id.as_str())
        .bind(serde_json::to_value(&handoff.contact.scope)?)
        .bind(handoff.accepted_at)
        .bind(&payload)
        .execute(&mut **tx)
        .await?;

        let receipt = CrmWriteReceipt {
            aggregate_type: HANDOFF_AGGREGATE.into(),
            aggregate_id: handoff.id,
            reference: handoff.reference.clone(),
            version: 1,
            replayed: false,
        };
        write_crm_source_event(
            tx,
            data_profile_key,
            HANDOFF_AGGREGATE,
            handoff.id,
            1,
            None,
            &payload,
            HANDOFF_EVENT,
            context,
        )
        .await?;
        insert_idempotency(
            tx,
            data_profile_key,
            HANDOFF_AGGREGATE,
            handoff.id,
            &handoff.reference,
            &handoff.idempotency_key,
            &request_hash,
            &serde_json::to_value(&receipt)?,
        )
        .await?;
        Ok(receipt)
    }

    /// Persist a CRM opportunity or one validated lifecycle transition.
    ///
    /// The accepted lead is resolved from the immutable handoff row.  The
    /// opportunity's source binding, account/contact references, commercial
    /// facts, and attribution inputs cannot change after creation; only the
    /// pure domain lifecycle transition can update status, command metadata,
    /// and updated_at.
    pub async fn save_opportunity(
        &self,
        opportunity: &CrmOpportunity,
        context: &CommandContext,
        expected_version: Option<u64>,
    ) -> Result<CrmWriteReceipt, PersistenceError> {
        validate_opportunity_command(opportunity, context)?;
        let payload = serde_json::to_value(opportunity)?;
        let request_hash = hash_request(&payload, expected_version)?;
        let (holding_id, company_id, branch_id) = scope_columns(&opportunity.scope)?;
        let attribution_hash = hash_value(&serde_json::to_value(opportunity.attribution())?)?;

        let mut tx = self.begin_write().await?;
        lock_command_keys(
            &mut tx,
            &self.data_profile_key,
            OPPORTUNITY_AGGREGATE,
            opportunity.id,
            &opportunity.reference,
            &opportunity.idempotency_key,
        )
        .await?;
        if let Some(binding) = load_idempotency(
            &mut tx,
            &self.data_profile_key,
            &opportunity.idempotency_key,
        )
        .await?
        {
            validate_idempotency_binding(
                &binding,
                OPPORTUNITY_AGGREGATE,
                opportunity.id,
                &opportunity.reference,
                &request_hash,
                &opportunity.idempotency_key,
            )?;
            let mut receipt: CrmWriteReceipt = serde_json::from_value(binding.response)?;
            validate_write_receipt(
                &receipt,
                OPPORTUNITY_AGGREGATE,
                opportunity.id,
                &opportunity.reference,
            )?;
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }

        let source_handoff_id =
            load_and_validate_handoff_binding(&mut tx, &self.data_profile_key, opportunity).await?;
        reject_existing_reference(
            &mut tx,
            &self.data_profile_key,
            OPPORTUNITY_AGGREGATE,
            &opportunity.reference,
            opportunity.id,
        )
        .await?;
        reject_existing_key_row(
            &mut tx,
            &self.data_profile_key,
            OPPORTUNITY_AGGREGATE,
            &opportunity.idempotency_key,
            opportunity.id,
        )
        .await?;

        let current: Option<(i64, Value, String)> = sqlx::query_as(
            "SELECT version, payload, attribution_hash
             FROM erpx_holding.crm_opportunities
             WHERE data_profile_key=$1 AND opportunity_id=$2
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(opportunity.id)
        .fetch_optional(&mut *tx)
        .await?;
        let (version, is_update) = match current {
            None => {
                if expected_version.is_some() {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: OPPORTUNITY_AGGREGATE.into(),
                        aggregate_id: opportunity.id,
                        expected: expected_version,
                        actual: None,
                    });
                }
                (1, false)
            }
            Some((stored_version, stored_payload, stored_attribution_hash)) => {
                let actual = as_u64(stored_version, "CRM opportunity version")?;
                let current: CrmOpportunity = serde_json::from_value(stored_payload)?;
                current.validate().map_err(domain_error)?;
                if stored_attribution_hash != attribution_hash {
                    return Err(PersistenceError::InvalidData(
                        "CRM opportunity attribution inputs are immutable".into(),
                    ));
                }
                if expected_version != Some(actual) {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: OPPORTUNITY_AGGREGATE.into(),
                        aggregate_id: opportunity.id,
                        expected: expected_version,
                        actual: Some(actual),
                    });
                }
                validate_opportunity_transition(&current, opportunity, context)?;
                let next = actual.checked_add(1).ok_or_else(|| {
                    PersistenceError::InvalidData("CRM opportunity version overflow".into())
                })?;
                (next, true)
            }
        };

        if is_update {
            sqlx::query(
                "UPDATE erpx_holding.crm_opportunities
                 SET holding_id=$3, company_id=$4, branch_id=$5, scope=$6,
                     idempotency_key=$7, command_id=$8, status=$9, updated_at=$10,
                     version=$11, payload=$12
                 WHERE data_profile_key=$1 AND opportunity_id=$2 AND version=$13",
            )
            .bind(&self.data_profile_key)
            .bind(opportunity.id)
            .bind(holding_id)
            .bind(company_id)
            .bind(branch_id)
            .bind(serde_json::to_value(&opportunity.scope)?)
            .bind(&opportunity.idempotency_key)
            .bind(opportunity.command_id)
            .bind(status_text(opportunity.status))
            .bind(opportunity.updated_at)
            .bind(as_i64(version, "CRM opportunity version")?)
            .bind(&payload)
            .bind(as_i64(version - 1, "CRM opportunity version")?)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.crm_opportunities
                    (opportunity_id, data_profile_key, holding_id, company_id, branch_id,
                     scope, reference, idempotency_key, command_id,
                     accepted_lead_handoff_id, accepted_lead_reference, account_id,
                     account_scope, contact_id, contact_scope, name, currency,
                     expected_revenue_minor, status, created_at, updated_at, version,
                     attribution_hash, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                         $18,$19,$20,$21,1,$22,$23)",
            )
            .bind(opportunity.id)
            .bind(&self.data_profile_key)
            .bind(holding_id)
            .bind(company_id)
            .bind(branch_id)
            .bind(serde_json::to_value(&opportunity.scope)?)
            .bind(&opportunity.reference)
            .bind(&opportunity.idempotency_key)
            .bind(opportunity.command_id)
            .bind(source_handoff_id)
            .bind(opportunity.accepted_lead.id.as_str())
            .bind(opportunity.account.id.as_str())
            .bind(serde_json::to_value(&opportunity.account.scope)?)
            .bind(opportunity.contact.id.as_str())
            .bind(serde_json::to_value(&opportunity.contact.scope)?)
            .bind(&opportunity.name)
            .bind(&opportunity.currency)
            .bind(opportunity.expected_revenue_minor)
            .bind(status_text(opportunity.status))
            .bind(opportunity.created_at)
            .bind(opportunity.updated_at)
            .bind(&attribution_hash)
            .bind(&payload)
            .execute(&mut *tx)
            .await?;
            insert_attribution_inputs(&mut tx, &self.data_profile_key, opportunity).await?;
        }

        let receipt = CrmWriteReceipt {
            aggregate_type: OPPORTUNITY_AGGREGATE.into(),
            aggregate_id: opportunity.id,
            reference: opportunity.reference.clone(),
            version,
            replayed: false,
        };
        write_crm_source_event(
            &mut tx,
            &self.data_profile_key,
            OPPORTUNITY_AGGREGATE,
            opportunity.id,
            version,
            is_update.then_some(version - 1),
            &payload,
            OPPORTUNITY_EVENT,
            context,
        )
        .await?;
        insert_idempotency(
            &mut tx,
            &self.data_profile_key,
            OPPORTUNITY_AGGREGATE,
            opportunity.id,
            &opportunity.reference,
            &opportunity.idempotency_key,
            &request_hash,
            &serde_json::to_value(&receipt)?,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Recompute and persist one immutable attribution result for the stored
    /// opportunity.  The supplied opportunity version is an optimistic read
    /// binding; the result is never accepted from caller-supplied totals.
    pub async fn record_attribution(
        &self,
        opportunity_id: Uuid,
        model: AttributionModel,
        context: &CommandContext,
        expected_opportunity_version: u64,
    ) -> Result<CrmAttributionWriteReceipt, PersistenceError> {
        validate_uuid(opportunity_id, "opportunity_id")?;
        validate_command_context(context).map_err(domain_error)?;
        validate_bounded_context_key(&context.idempotency_key)?;
        if matches!(context.active_scope, AuthorityScope::Platform) {
            return Err(PersistenceError::InvalidData(
                "CRM attribution requires a non-Platform command scope".into(),
            ));
        }

        let mut tx = self.begin_write().await?;
        lock_command_keys(
            &mut tx,
            &self.data_profile_key,
            ATTRIBUTION_AGGREGATE,
            opportunity_id,
            &format!("CRM-ATTR-{}", opportunity_id.simple()),
            &context.idempotency_key,
        )
        .await?;
        let row = sqlx::query(
            "SELECT version, payload
             FROM erpx_holding.crm_opportunities
             WHERE data_profile_key=$1 AND opportunity_id=$2
             FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(opportunity_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "crm_opportunity",
            id: opportunity_id,
        })?;
        let opportunity_version = as_u64(row.get::<i64, _>("version"), "CRM opportunity version")?;
        let opportunity: CrmOpportunity = serde_json::from_value(row.get("payload"))?;
        opportunity.validate().map_err(domain_error)?;
        if !context.active_scope.contains(&opportunity.scope) {
            return Err(PersistenceError::InvalidData(
                "CRM attribution command scope is outside the opportunity scope".into(),
            ));
        }
        let result = opportunity
            .attribution()
            .attribute(model, opportunity.expected_revenue_minor)
            .map_err(domain_error)?;
        let model_text = attribution_model_text(model);
        let record_id = deterministic_record_id(&self.data_profile_key, opportunity_id, model_text);
        let reference = format!(
            "CRM-ATTR-{}",
            record_id.simple().to_string().to_ascii_uppercase()
        );
        let payload = json!({
            "record_id": record_id,
            "opportunity_id": opportunity_id,
            "source_opportunity_version": expected_opportunity_version,
            "model": model,
            "result": result,
        });
        let request_hash = hash_request(&payload, Some(expected_opportunity_version))?;
        if let Some(binding) =
            load_idempotency(&mut tx, &self.data_profile_key, &context.idempotency_key).await?
        {
            validate_idempotency_binding(
                &binding,
                ATTRIBUTION_AGGREGATE,
                record_id,
                &reference,
                &request_hash,
                &context.idempotency_key,
            )?;
            let mut receipt: CrmAttributionWriteReceipt = serde_json::from_value(binding.response)?;
            validate_attribution_receipt(
                &receipt,
                record_id,
                &reference,
                model_text,
                expected_opportunity_version,
            )?;
            receipt.replayed = true;
            if receipt.source_opportunity_version != expected_opportunity_version {
                return Err(PersistenceError::IdempotencyConflict {
                    key: context.idempotency_key.clone(),
                });
            }
            reconcile_attribution_in_transaction(
                &mut tx,
                &self.data_profile_key,
                opportunity_id,
                model,
                Some(&context.idempotency_key),
                Some(expected_opportunity_version),
            )
            .await?;
            tx.commit().await?;
            return Ok(receipt);
        }
        if opportunity_version != expected_opportunity_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: OPPORTUNITY_AGGREGATE.into(),
                aggregate_id: opportunity_id,
                expected: Some(expected_opportunity_version),
                actual: Some(opportunity_version),
            });
        }
        let duplicate_model: Option<Uuid> = sqlx::query_scalar(
            "SELECT record_id
             FROM erpx_holding.crm_attribution_records
             WHERE data_profile_key=$1 AND opportunity_id=$2 AND model=$3
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(opportunity_id)
        .bind(model_text)
        .fetch_optional(&mut *tx)
        .await?;
        if duplicate_model.is_some() {
            return Err(PersistenceError::InvalidData(
                "CRM attribution model is already recorded for this opportunity".into(),
            ));
        }
        reject_existing_key_row(
            &mut tx,
            &self.data_profile_key,
            ATTRIBUTION_AGGREGATE,
            &context.idempotency_key,
            record_id,
        )
        .await?;
        sqlx::query(
            "INSERT INTO erpx_holding.crm_attribution_records
                (record_id, data_profile_key, opportunity_id, model, idempotency_key,
                 source_opportunity_version, version, total_revenue_minor,
                 total_cost_minor, payload)
             VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8,$9)",
        )
        .bind(record_id)
        .bind(&self.data_profile_key)
        .bind(opportunity_id)
        .bind(model_text)
        .bind(&context.idempotency_key)
        .bind(as_i64(opportunity_version, "CRM opportunity version")?)
        .bind(result.total_revenue_minor)
        .bind(result.total_cost_minor)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        for entry in &result.entries {
            sqlx::query(
                "INSERT INTO erpx_holding.crm_attribution_entries
                    (data_profile_key, record_id, touch_id, source_id, campaign_id,
                     revenue_minor, cost_minor)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(&self.data_profile_key)
            .bind(record_id)
            .bind(entry.touch_id.as_str())
            .bind(entry.source_id.as_str())
            .bind(entry.campaign_id.as_ref().map(|value| value.as_str()))
            .bind(entry.revenue_minor)
            .bind(entry.cost_minor)
            .execute(&mut *tx)
            .await?;
        }
        let receipt = CrmAttributionWriteReceipt {
            record_id,
            reference: reference.clone(),
            model: model_text.into(),
            source_opportunity_version: opportunity_version,
            version: 1,
            replayed: false,
        };
        write_crm_source_event(
            &mut tx,
            &self.data_profile_key,
            ATTRIBUTION_AGGREGATE,
            record_id,
            1,
            None,
            &payload,
            ATTRIBUTION_EVENT,
            context,
        )
        .await?;
        insert_idempotency(
            &mut tx,
            &self.data_profile_key,
            ATTRIBUTION_AGGREGATE,
            record_id,
            &reference,
            &context.idempotency_key,
            &request_hash,
            &serde_json::to_value(&receipt)?,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Reconcile one persisted attribution against its immutable opportunity
    /// inputs and the source event that feeds the CRM read model.
    ///
    /// This is deliberately read-only and holding-scoped.  It recomputes the
    /// allocation from the authoritative opportunity payload, then checks the
    /// normalized entry rows and exactly one complete outbox envelope. A
    /// missing or divergent source artifact fails closed;
    /// callers can then queue the existing projection/rebuild workflow without
    /// accepting a fabricated attribution result.
    pub async fn reconcile_attribution(
        &self,
        opportunity_id: Uuid,
        model: AttributionModel,
    ) -> Result<(), PersistenceError> {
        validate_uuid(opportunity_id, "opportunity_id")?;
        let model_text = attribution_model_text(model);
        let record_id = deterministic_record_id(&self.data_profile_key, opportunity_id, model_text);
        let mut tx = self.begin_write().await?;

        let opportunity_row = sqlx::query(
            "SELECT version, payload
             FROM erpx_holding.crm_opportunities
             WHERE data_profile_key=$1 AND opportunity_id=$2
             FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(opportunity_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "crm_opportunity",
            id: opportunity_id,
        })?;
        let opportunity_version = as_u64(
            opportunity_row.get::<i64, _>("version"),
            "CRM opportunity version",
        )?;
        let opportunity: CrmOpportunity = serde_json::from_value(opportunity_row.get("payload"))?;
        opportunity.validate().map_err(domain_error)?;
        let expected_result = opportunity
            .attribution()
            .attribute(model, opportunity.expected_revenue_minor)
            .map_err(domain_error)?;

        let record_row = sqlx::query(
            "SELECT record_id, model, idempotency_key, source_opportunity_version,
                    version, total_revenue_minor, total_cost_minor, payload
             FROM erpx_holding.crm_attribution_records
             WHERE data_profile_key=$1 AND opportunity_id=$2 AND model=$3
             FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(opportunity_id)
        .bind(model_text)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "crm_attribution_record",
            id: record_id,
        })?;
        let stored_record_id: Uuid = record_row.get("record_id");
        let stored_model: String = record_row.get("model");
        let stored_idempotency_key: String = record_row.get("idempotency_key");
        let source_opportunity_version = as_u64(
            record_row.get::<i64, _>("source_opportunity_version"),
            "CRM attribution source opportunity version",
        )?;
        let stored_version = as_u64(
            record_row.get::<i64, _>("version"),
            "CRM attribution version",
        )?;
        let stored_payload: StoredAttributionPayload =
            serde_json::from_value(record_row.get("payload"))?;

        if stored_record_id != record_id
            || stored_model != model_text
            || stored_version != 1
            || source_opportunity_version == 0
            || source_opportunity_version > opportunity_version
            || stored_payload.record_id != record_id
            || stored_payload.opportunity_id != opportunity_id
            || stored_payload.source_opportunity_version != source_opportunity_version
            || stored_payload.model != model
            || stored_payload.result != expected_result
            || record_row.get::<i64, _>("total_revenue_minor")
                != expected_result.total_revenue_minor
            || record_row.get::<i64, _>("total_cost_minor") != expected_result.total_cost_minor
        {
            return Err(PersistenceError::InvalidData(
                "CRM attribution authoritative record is inconsistent".into(),
            ));
        }

        let stored_payload_value: Value = record_row.get("payload");
        let request_hash = hash_request(&stored_payload_value, Some(source_opportunity_version))?;
        let binding = load_idempotency(&mut tx, &self.data_profile_key, &stored_idempotency_key)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "CRM attribution idempotency binding is missing".into(),
                )
            })?;
        validate_idempotency_binding(
            &binding,
            ATTRIBUTION_AGGREGATE,
            record_id,
            &format!(
                "CRM-ATTR-{}",
                record_id.simple().to_string().to_ascii_uppercase()
            ),
            &request_hash,
            &stored_idempotency_key,
        )?;
        let stored_receipt: CrmAttributionWriteReceipt = serde_json::from_value(binding.response)?;
        validate_attribution_receipt(
            &stored_receipt,
            record_id,
            &format!(
                "CRM-ATTR-{}",
                record_id.simple().to_string().to_ascii_uppercase()
            ),
            model_text,
            source_opportunity_version,
        )?;

        let persisted_entries = sqlx::query(
            "SELECT touch_id, source_id, campaign_id, revenue_minor, cost_minor
             FROM erpx_holding.crm_attribution_entries
             WHERE data_profile_key=$1 AND record_id=$2
             ORDER BY touch_id",
        )
        .bind(&self.data_profile_key)
        .bind(record_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| StoredAttributionEntry {
            touch_id: row.get("touch_id"),
            source_id: row.get("source_id"),
            campaign_id: row.get("campaign_id"),
            revenue_minor: row.get("revenue_minor"),
            cost_minor: row.get("cost_minor"),
        })
        .collect::<Vec<_>>();
        let mut expected_entries = expected_result
            .entries
            .iter()
            .map(|entry| StoredAttributionEntry {
                touch_id: entry.touch_id.as_str().to_owned(),
                source_id: entry.source_id.as_str().to_owned(),
                campaign_id: entry
                    .campaign_id
                    .as_ref()
                    .map(|campaign| campaign.as_str().to_owned()),
                revenue_minor: entry.revenue_minor,
                cost_minor: entry.cost_minor,
            })
            .collect::<Vec<_>>();
        expected_entries.sort_by(|left, right| left.touch_id.cmp(&right.touch_id));
        if persisted_entries != expected_entries {
            return Err(PersistenceError::InvalidData(
                "CRM attribution entry rows diverge from authoritative inputs".into(),
            ));
        }

        let outbox_rows = sqlx::query(
            "SELECT contract_version, event_version, idempotency_key,
                    active_scope, actor, correlation_id, causation_id, payload
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3
               AND aggregate_version=1 AND event_type=$4
             FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(ATTRIBUTION_AGGREGATE)
        .bind(record_id)
        .bind(ATTRIBUTION_EVENT)
        .fetch_all(&mut *tx)
        .await?;
        if outbox_rows.len() != 1 {
            return Err(PersistenceError::InvalidData(
                "CRM attribution source event count is not exactly one".into(),
            ));
        }
        let outbox = &outbox_rows[0];
        let event_scope: AuthorityScope = outbox
            .get::<Option<Value>, _>("active_scope")
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "CRM attribution source event holding scope is missing".into(),
                )
            })
            .and_then(|value| {
                serde_json::from_value::<AuthorityScope>(value).map_err(PersistenceError::from)
            })?;
        let event_correlation_id: Uuid = outbox
            .get::<Option<Uuid>, _>("correlation_id")
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "CRM attribution source event correlation is missing".into(),
                )
            })?;
        let expected_event_payload = json!({
            "aggregate_type": ATTRIBUTION_AGGREGATE,
            "aggregate": record_row.get::<Value, _>("payload"),
            "version": 1,
            "event_type": ATTRIBUTION_EVENT,
        });
        if outbox.get::<i32, _>("contract_version") != i32::from(CONTRACT_VERSION)
            || outbox.get::<i32, _>("event_version") != 1
            || outbox
                .get::<Option<String>, _>("idempotency_key")
                .as_deref()
                != Some(stored_idempotency_key.as_str())
            || outbox.get::<Option<Value>, _>("actor").is_none()
            || matches!(event_scope, AuthorityScope::Platform)
            || !event_scope.contains(&opportunity.scope)
            || event_correlation_id.is_nil()
            || outbox.get::<Value, _>("payload") != expected_event_payload
        {
            return Err(PersistenceError::InvalidData(
                "CRM attribution source event is incomplete, unauthorized, or divergent".into(),
            ));
        }

        tx.commit().await?;
        Ok(())
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.data_profile_key).await
    }
}

/// Revalidate the authoritative artifacts before an attribution replay is
/// acknowledged.  The normal reconciliation method owns its transaction;
/// replay uses this transaction-local form so a missing or unauthorized
/// source event cannot be hidden behind a previously stored receipt.
async fn reconcile_attribution_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    opportunity_id: Uuid,
    model: AttributionModel,
    expected_idempotency_key: Option<&str>,
    expected_source_opportunity_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let model_text = attribution_model_text(model);
    let record_id = deterministic_record_id(data_profile_key, opportunity_id, model_text);
    let opportunity_row = sqlx::query(
        "SELECT version, payload
         FROM erpx_holding.crm_opportunities
         WHERE data_profile_key=$1 AND opportunity_id=$2
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(opportunity_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "crm_opportunity",
        id: opportunity_id,
    })?;
    let opportunity_version = as_u64(
        opportunity_row.get::<i64, _>("version"),
        "CRM opportunity version",
    )?;
    let opportunity: CrmOpportunity = serde_json::from_value(opportunity_row.get("payload"))?;
    opportunity.validate().map_err(domain_error)?;
    let expected_result = opportunity
        .attribution()
        .attribute(model, opportunity.expected_revenue_minor)
        .map_err(domain_error)?;

    let record_row = sqlx::query(
        "SELECT record_id, model, idempotency_key, source_opportunity_version,
                version, total_revenue_minor, total_cost_minor, payload
         FROM erpx_holding.crm_attribution_records
         WHERE data_profile_key=$1 AND opportunity_id=$2 AND model=$3
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(opportunity_id)
    .bind(model_text)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "crm_attribution_record",
        id: record_id,
    })?;
    let stored_record_id: Uuid = record_row.get("record_id");
    let stored_model: String = record_row.get("model");
    let stored_idempotency_key: String = record_row.get("idempotency_key");
    let source_opportunity_version = as_u64(
        record_row.get::<i64, _>("source_opportunity_version"),
        "CRM attribution source opportunity version",
    )?;
    let stored_version = as_u64(
        record_row.get::<i64, _>("version"),
        "CRM attribution version",
    )?;
    let stored_payload: StoredAttributionPayload =
        serde_json::from_value(record_row.get("payload"))?;
    if stored_record_id != record_id
        || stored_model != model_text
        || stored_version != 1
        || source_opportunity_version == 0
        || source_opportunity_version > opportunity_version
        || expected_source_opportunity_version
            .is_some_and(|version| version != source_opportunity_version)
        || expected_idempotency_key.is_some_and(|key| key != stored_idempotency_key)
        || stored_payload.record_id != record_id
        || stored_payload.opportunity_id != opportunity_id
        || stored_payload.source_opportunity_version != source_opportunity_version
        || stored_payload.model != model
        || stored_payload.result != expected_result
        || record_row.get::<i64, _>("total_revenue_minor") != expected_result.total_revenue_minor
        || record_row.get::<i64, _>("total_cost_minor") != expected_result.total_cost_minor
    {
        return Err(PersistenceError::InvalidData(
            "CRM attribution replay source is inconsistent".into(),
        ));
    }

    let outbox_rows = sqlx::query(
        "SELECT contract_version, event_version, idempotency_key,
                active_scope, actor, correlation_id, payload
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3
           AND aggregate_version=1 AND event_type=$4
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(ATTRIBUTION_AGGREGATE)
    .bind(record_id)
    .bind(ATTRIBUTION_EVENT)
    .fetch_all(&mut **tx)
    .await?;
    if outbox_rows.len() != 1 {
        return Err(PersistenceError::InvalidData(
            "CRM attribution source event count is not exactly one".into(),
        ));
    }
    let outbox = &outbox_rows[0];
    let event_scope: AuthorityScope = outbox
        .get::<Option<Value>, _>("active_scope")
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "CRM attribution source event holding scope is missing".into(),
            )
        })
        .and_then(|value| {
            serde_json::from_value::<AuthorityScope>(value).map_err(PersistenceError::from)
        })?;
    let event_correlation_id: Uuid =
        outbox
            .get::<Option<Uuid>, _>("correlation_id")
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "CRM attribution source event correlation is missing".into(),
                )
            })?;
    let expected_event_payload = json!({
        "aggregate_type": ATTRIBUTION_AGGREGATE,
        "aggregate": record_row.get::<Value, _>("payload"),
        "version": 1,
        "event_type": ATTRIBUTION_EVENT,
    });
    if outbox.get::<i32, _>("contract_version") != i32::from(CONTRACT_VERSION)
        || outbox.get::<i32, _>("event_version") != 1
        || outbox
            .get::<Option<String>, _>("idempotency_key")
            .as_deref()
            != Some(stored_idempotency_key.as_str())
        || outbox.get::<Option<Value>, _>("actor").is_none()
        || matches!(event_scope, AuthorityScope::Platform)
        || !event_scope.contains(&opportunity.scope)
        || event_correlation_id.is_nil()
        || outbox.get::<Value, _>("payload") != expected_event_payload
    {
        return Err(PersistenceError::InvalidData(
            "CRM attribution source event is incomplete, unauthorized, or divergent".into(),
        ));
    }
    Ok(())
}

/// Persist the CRM source event as one complete shared event envelope. The
/// caller has already fenced and persisted the normalized owner row; this
/// helper validates contiguous event history and appends the rebuild fact
/// without maintaining a second generic aggregate authority.
#[allow(clippy::too_many_arguments)]
async fn write_crm_source_event(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    expected_previous_version: Option<u64>,
    payload: &Value,
    event_type: &str,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    if context.contract_version != CONTRACT_VERSION {
        return Err(PersistenceError::InvalidData(
            "CRM event contract version is unsupported".into(),
        ));
    }
    if let Some(previous_version) = expected_previous_version {
        let previous_version = as_i64(previous_version, "CRM aggregate version")?;
        let (
            source_event_count,
            matching_source_event_count,
            distinct_source_versions,
            first_source_version,
            last_source_version,
        ): (i64, i64, i64, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT count(*)::BIGINT,
                    count(*) FILTER (WHERE event_type=$5)::BIGINT,
                    count(DISTINCT aggregate_version)::BIGINT,
                    min(aggregate_version), max(aggregate_version)
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND aggregate_type=$2 AND aggregate_id=$3
               AND aggregate_version BETWEEN 1 AND $4",
        )
        .bind(data_profile_key)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(previous_version)
        .bind(event_type)
        .fetch_one(&mut **tx)
        .await?;
        validate_source_event_history(
            previous_version,
            source_event_count,
            matching_source_event_count,
            distinct_source_versions,
            first_source_version,
            last_source_version,
        )?;
    }
    let version = as_i64(version, "CRM aggregate version")?;
    let event_id = Uuid::new_v4();
    let event_payload = json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
        "version": u64::try_from(version).map_err(|_| {
            PersistenceError::InvalidData("CRM aggregate version is negative".into())
        })?,
        "event_type": event_type,
    });
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, active_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now(),$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(event_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(data_profile_key)
    .bind(version)
    .bind(event_type)
    .bind(event_payload)
    .bind(i32::from(context.contract_version))
    .bind(1_i32)
    .bind(serde_json::to_value(&context.active_scope)?)
    .bind(serde_json::to_value(&context.actor)?)
    .bind(context.correlation_id)
    .bind(context.causation_id)
    .bind(Some(context.idempotency_key.clone()))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_source_event_history(
    previous_version: i64,
    source_event_count: i64,
    matching_source_event_count: i64,
    distinct_source_versions: i64,
    first_source_version: Option<i64>,
    last_source_version: Option<i64>,
) -> Result<(), PersistenceError> {
    if previous_version <= 0
        || source_event_count != previous_version
        || matching_source_event_count != previous_version
        || distinct_source_versions != previous_version
        || first_source_version != Some(1)
        || last_source_version != Some(previous_version)
    {
        return Err(PersistenceError::InvalidData(
            "CRM aggregate source event history is not contiguous and type-safe".into(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct IdempotencyBinding {
    aggregate_type: String,
    aggregate_id: Uuid,
    reference: String,
    request_hash: String,
    response: Value,
}

#[derive(Debug, Deserialize)]
struct StoredAttributionPayload {
    record_id: Uuid,
    opportunity_id: Uuid,
    source_opportunity_version: u64,
    model: AttributionModel,
    result: AttributionResult,
}

#[derive(Debug, PartialEq, Eq)]
struct StoredAttributionEntry {
    touch_id: String,
    source_id: String,
    campaign_id: Option<String>,
    revenue_minor: i64,
    cost_minor: i64,
}

async fn load_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    key: &str,
) -> Result<Option<IdempotencyBinding>, PersistenceError> {
    Ok(sqlx::query(
        "SELECT aggregate_type, aggregate_id, reference, request_hash, response
         FROM erpx_holding.crm_idempotency_bindings
         WHERE data_profile_key=$1 AND idempotency_key=$2
         FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(key)
    .fetch_optional(&mut **tx)
    .await?
    .map(|row| IdempotencyBinding {
        aggregate_type: row.get("aggregate_type"),
        aggregate_id: row.get("aggregate_id"),
        reference: row.get("reference"),
        request_hash: row.get("request_hash"),
        response: row.get("response"),
    }))
}

fn validate_idempotency_binding(
    binding: &IdempotencyBinding,
    aggregate_type: &str,
    aggregate_id: Uuid,
    reference: &str,
    request_hash: &str,
    key: &str,
) -> Result<(), PersistenceError> {
    if binding.aggregate_type != aggregate_type
        || binding.aggregate_id != aggregate_id
        || binding.reference != reference
        || binding.request_hash != request_hash
    {
        return Err(PersistenceError::IdempotencyConflict { key: key.into() });
    }
    Ok(())
}

fn validate_write_receipt(
    receipt: &CrmWriteReceipt,
    aggregate_type: &str,
    aggregate_id: Uuid,
    reference: &str,
) -> Result<(), PersistenceError> {
    if receipt.aggregate_type != aggregate_type
        || receipt.aggregate_id != aggregate_id
        || receipt.reference != reference
        || receipt.version == 0
        || receipt.replayed
    {
        return Err(PersistenceError::InvalidData(
            "CRM idempotency receipt is divergent from its binding".into(),
        ));
    }
    Ok(())
}

fn validate_attribution_receipt(
    receipt: &CrmAttributionWriteReceipt,
    record_id: Uuid,
    reference: &str,
    model: &str,
    source_opportunity_version: u64,
) -> Result<(), PersistenceError> {
    if receipt.record_id != record_id
        || receipt.reference != reference
        || receipt.model != model
        || receipt.source_opportunity_version != source_opportunity_version
        || receipt.version != 1
        || receipt.replayed
    {
        return Err(PersistenceError::InvalidData(
            "CRM attribution idempotency receipt is divergent from its binding".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    reference: &str,
    idempotency_key: &str,
    request_hash: &str,
    response: &Value,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.crm_idempotency_bindings
            (data_profile_key, idempotency_key, aggregate_type, aggregate_id,
             reference, request_hash, response)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(data_profile_key)
    .bind(idempotency_key)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(reference)
    .bind(request_hash)
    .bind(response)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn reject_existing_reference(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    reference: &str,
    aggregate_id: Uuid,
) -> Result<(), PersistenceError> {
    let table = match aggregate_type {
        HANDOFF_AGGREGATE => "crm_accepted_lead_handoffs",
        OPPORTUNITY_AGGREGATE => "crm_opportunities",
        _ => return Ok(()),
    };
    let statement = format!(
        "SELECT 1 FROM erpx_holding.{table}
         WHERE data_profile_key=$1 AND reference=$2 AND {} <> $3
         FOR UPDATE",
        if aggregate_type == HANDOFF_AGGREGATE {
            "handoff_id"
        } else {
            "opportunity_id"
        }
    );
    if sqlx::query(&statement)
        .bind(data_profile_key)
        .bind(reference)
        .bind(aggregate_id)
        .fetch_optional(&mut **tx)
        .await?
        .is_some()
    {
        return Err(PersistenceError::InvalidData(format!(
            "CRM reference {reference} is already bound"
        )));
    }
    Ok(())
}

async fn reject_existing_key_row(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    key: &str,
    aggregate_id: Uuid,
) -> Result<(), PersistenceError> {
    let table = match aggregate_type {
        HANDOFF_AGGREGATE => "crm_accepted_lead_handoffs",
        OPPORTUNITY_AGGREGATE => "crm_opportunities",
        ATTRIBUTION_AGGREGATE => "crm_attribution_records",
        _ => return Ok(()),
    };
    let id_column = match aggregate_type {
        HANDOFF_AGGREGATE => "handoff_id",
        OPPORTUNITY_AGGREGATE => "opportunity_id",
        ATTRIBUTION_AGGREGATE => "record_id",
        _ => unreachable!(),
    };
    let statement = format!(
        "SELECT {id_column} FROM erpx_holding.{table}
         WHERE data_profile_key=$1 AND idempotency_key=$2
         FOR UPDATE"
    );
    if let Some(row) = sqlx::query(&statement)
        .bind(data_profile_key)
        .bind(key)
        .fetch_optional(&mut **tx)
        .await?
    {
        let existing: Uuid = row.get(id_column);
        if existing != aggregate_id {
            return Err(PersistenceError::IdempotencyConflict { key: key.into() });
        }
    }
    Ok(())
}

async fn lock_command_keys(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    reference: &str,
    idempotency_key: &str,
) -> Result<(), PersistenceError> {
    for key in [
        format!("{data_profile_key}:crm:{aggregate_type}:id:{aggregate_id}"),
        format!("{data_profile_key}:crm:{aggregate_type}:reference:{reference}"),
        format!("{data_profile_key}:crm:idempotency:{idempotency_key}"),
    ] {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(key)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn load_and_validate_handoff_binding(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    opportunity: &CrmOpportunity,
) -> Result<Uuid, PersistenceError> {
    let row = sqlx::query(
        "SELECT handoff_id, payload
         FROM erpx_holding.crm_accepted_lead_handoffs
         WHERE data_profile_key=$1 AND reference=$2
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(opportunity.accepted_lead.id.as_str())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        PersistenceError::InvalidData(
            "CRM opportunity references an unpersisted accepted lead handoff".into(),
        )
    })?;
    let handoff_id: Uuid = row.get("handoff_id");
    let handoff: AcceptedLeadHandoff = serde_json::from_value(row.get("payload"))?;
    handoff.validate().map_err(domain_error)?;
    if handoff.reference != opportunity.accepted_lead.id.as_str()
        || !handoff.scope.contains(&opportunity.scope)
        || handoff.account != opportunity.account
        || handoff.contact != opportunity.contact
    {
        return Err(PersistenceError::InvalidData(
            "CRM opportunity accepted-lead source identity binding is invalid".into(),
        ));
    }
    Ok(handoff_id)
}

async fn insert_attribution_inputs(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    opportunity: &CrmOpportunity,
) -> Result<(), PersistenceError> {
    for touch in opportunity.attribution().touches() {
        sqlx::query(
            "INSERT INTO erpx_holding.crm_opportunity_attribution_inputs
                (data_profile_key, opportunity_id, touch_id, source_id, campaign_id,
                 scope, occurred_at, cost_minor, weight_bps)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(data_profile_key)
        .bind(opportunity.id)
        .bind(touch.touch_id.as_str())
        .bind(touch.source_id.as_str())
        .bind(touch.campaign_id.as_ref().map(|value| value.as_str()))
        .bind(serde_json::to_value(&touch.scope)?)
        .bind(touch.occurred_at)
        .bind(touch.cost_minor)
        .bind(touch.weight_bps.map(i32::from))
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn validate_handoff_command(
    handoff: &AcceptedLeadHandoff,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    handoff.validate().map_err(domain_error)?;
    handoff
        .validate_for_command(context)
        .map_err(domain_error)?;
    let expected = deterministic_idempotency_key(
        erpx_domain::marketing_crm::CrmAggregateKind::AcceptedLead,
        context,
    )
    .map_err(domain_error)?;
    if handoff.command_id != context.command_id || handoff.idempotency_key != expected {
        return Err(PersistenceError::InvalidData(
            "CRM handoff command metadata does not match its command context".into(),
        ));
    }
    Ok(())
}

fn validate_opportunity_command(
    opportunity: &CrmOpportunity,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    opportunity.validate().map_err(domain_error)?;
    opportunity
        .validate_for_command(context)
        .map_err(domain_error)?;
    let expected = deterministic_idempotency_key(
        erpx_domain::marketing_crm::CrmAggregateKind::Opportunity,
        context,
    )
    .map_err(domain_error)?;
    if opportunity.command_id != context.command_id || opportunity.idempotency_key != expected {
        return Err(PersistenceError::InvalidData(
            "CRM opportunity command metadata does not match its command context".into(),
        ));
    }
    Ok(())
}

fn validate_opportunity_transition(
    current: &CrmOpportunity,
    candidate: &CrmOpportunity,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    let mut transitioned = current.clone();
    transitioned
        .transition(candidate.status, context, candidate.updated_at)
        .map_err(domain_error)?;
    if transitioned.status != candidate.status
        || transitioned.updated_at != candidate.updated_at
        || current.id != candidate.id
        || current.reference != candidate.reference
        || current.scope != candidate.scope
        || current.accepted_lead != candidate.accepted_lead
        || current.account != candidate.account
        || current.contact != candidate.contact
        || current.name != candidate.name
        || current.currency != candidate.currency
        || current.expected_revenue_minor != candidate.expected_revenue_minor
        || current.created_at != candidate.created_at
        || current.attribution() != candidate.attribution()
    {
        return Err(PersistenceError::InvalidData(
            "CRM opportunity transition attempted to mutate an immutable field".into(),
        ));
    }
    Ok(())
}

fn scope_columns(
    scope: &AuthorityScope,
) -> Result<(Uuid, Option<Uuid>, Option<Uuid>), PersistenceError> {
    match scope {
        AuthorityScope::Platform => Err(PersistenceError::InvalidData(
            "CRM records cannot use Platform scope".into(),
        )),
        AuthorityScope::Holding { holding_id } => {
            validate_uuid(*holding_id, "holding_id")?;
            Ok((*holding_id, None, None))
        }
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "holding_id")?;
            validate_uuid(*company_id, "company_id")?;
            Ok((*holding_id, Some(*company_id), None))
        }
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "holding_id")?;
            validate_uuid(*company_id, "company_id")?;
            validate_uuid(*branch_id, "branch_id")?;
            Ok((*holding_id, Some(*company_id), Some(*branch_id)))
        }
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => {
            validate_uuid(*holding_id, "holding_id")?;
            validate_uuid(*organization_node_id, "organization_node_id")?;
            Ok((*holding_id, None, None))
        }
    }
}

fn status_text(status: erpx_domain::marketing_crm::OpportunityStatus) -> &'static str {
    match status {
        erpx_domain::marketing_crm::OpportunityStatus::Draft => "draft",
        erpx_domain::marketing_crm::OpportunityStatus::Qualified => "qualified",
        erpx_domain::marketing_crm::OpportunityStatus::Won => "won",
        erpx_domain::marketing_crm::OpportunityStatus::Lost => "lost",
    }
}

fn attribution_model_text(model: AttributionModel) -> &'static str {
    match model {
        AttributionModel::FirstTouch => "first_touch",
        AttributionModel::LastTouch => "last_touch",
        AttributionModel::Linear => "linear",
        AttributionModel::UShaped => "u_shaped",
        AttributionModel::Weighted => "weighted",
    }
}

fn deterministic_record_id(data_profile_key: &str, opportunity_id: Uuid, model: &str) -> Uuid {
    let digest = Sha256::digest(
        format!("{data_profile_key}:crm-attribution:{opportunity_id}:{model}").as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn hash_request(
    payload: &Value,
    expected_version: Option<u64>,
) -> Result<String, PersistenceError> {
    hash_value(&json!({
        "payload": payload,
        "expected_version": expected_version,
    }))
}

fn hash_value(value: &Value) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

fn validate_bounded_context_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_CONTEXT_KEY_BYTES
        || value.chars().any(|character| character.is_control())
    {
        return Err(PersistenceError::InvalidData(
            "CRM idempotency key is empty or exceeds its bound".into(),
        ));
    }
    Ok(())
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| character.is_control())
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn domain_error(error: MarketingCrmError) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_columns_reject_platform_and_preserve_branch_scope() {
        assert!(scope_columns(&AuthorityScope::Platform).is_err());
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        assert_eq!(
            scope_columns(&AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            })
            .expect("branch scope"),
            (holding_id, Some(company_id), Some(branch_id))
        );
    }

    #[test]
    fn request_hash_binds_version_and_payload() {
        let payload = json!({"value": 1});
        assert_ne!(
            hash_request(&payload, None).expect("hash"),
            hash_request(&payload, Some(1)).expect("hash")
        );
        assert_ne!(
            hash_request(&payload, None).expect("hash"),
            hash_request(&json!({"value": 2}), None).expect("hash")
        );
    }

    #[test]
    fn attribution_model_text_is_stable() {
        assert_eq!(
            attribution_model_text(AttributionModel::UShaped),
            "u_shaped"
        );
        assert_eq!(
            status_text(erpx_domain::marketing_crm::OpportunityStatus::Won),
            "won"
        );
    }

    #[test]
    fn source_history_requires_exact_expected_event_type_per_version() {
        assert!(validate_source_event_history(1, 1, 0, 1, Some(1), Some(1)).is_err());
        assert!(validate_source_event_history(1, 2, 1, 1, Some(1), Some(1)).is_err());
        assert!(validate_source_event_history(2, 2, 2, 1, Some(1), Some(2)).is_err());
        validate_source_event_history(2, 2, 2, 2, Some(1), Some(2))
            .expect("each historical version must have one expected source event");
    }

    #[test]
    fn idempotency_binding_requires_matching_aggregate_identity_and_hash() {
        let aggregate_id = Uuid::new_v4();
        let binding = IdempotencyBinding {
            aggregate_type: OPPORTUNITY_AGGREGATE.into(),
            aggregate_id,
            reference: "CRM-OPP-REFERENCE".into(),
            request_hash: "request-hash".into(),
            response: json!({}),
        };
        assert!(
            validate_idempotency_binding(
                &binding,
                OPPORTUNITY_AGGREGATE,
                aggregate_id,
                "CRM-OPP-REFERENCE",
                "request-hash",
                "crm/opportunity/key",
            )
            .is_ok()
        );
        assert!(matches!(
            validate_idempotency_binding(
                &binding,
                HANDOFF_AGGREGATE,
                aggregate_id,
                "CRM-OPP-REFERENCE",
                "request-hash",
                "crm/accepted_lead/key",
            ),
            Err(PersistenceError::IdempotencyConflict { key })
                if key == "crm/accepted_lead/key"
        ));
        assert!(
            validate_idempotency_binding(
                &binding,
                OPPORTUNITY_AGGREGATE,
                aggregate_id,
                "CRM-OPP-REFERENCE",
                "different-request-hash",
                "crm/opportunity/key",
            )
            .is_err()
        );
    }
}
